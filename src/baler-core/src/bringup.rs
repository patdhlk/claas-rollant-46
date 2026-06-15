//! Transport bring-up policy (ISSUE_0011).
//!
//! `baler-ui` and `baler-ethercat` both `open_or_create` the shared iceoryx2
//! services at boot with no ordering, so they can race and corrupt a service
//! (`ServiceInCorruptedState`). The old hand-rolled relay logged once and gave
//! up, leaving the daemon deaf and mute for the rest of the process.
//!
//! [`retry_open`] is the *decision* logic for recovering from that race —
//! generic over the resource, error type, and a corruption classifier so it is
//! testable on the macOS host where iceoryx2 is not a dependency. The binaries
//! supply iceoryx2-specific open / cleanup / backoff closures.

/// Open a transport resource, retrying on failure up to `max_attempts`.
///
/// * On a corruption error (`is_corrupted` returns `true`) the `cleanup` hook is
///   invoked before the next attempt — the race leaves a corrupted service that
///   must be cleared before a fresh `open_or_create` can succeed.
/// * `backoff(attempt)` is called between attempts (the 1-based number of the
///   attempt that just failed) so the caller controls the sleep schedule.
/// * Returns the first `Ok`, or the last `Err` once attempts are exhausted.
pub fn retry_open<T, E>(
    max_attempts: u32,
    mut open: impl FnMut() -> Result<T, E>,
    is_corrupted: impl Fn(&E) -> bool,
    mut cleanup: impl FnMut(),
    mut backoff: impl FnMut(u32),
) -> Result<T, E> {
    let mut attempt = 1;
    loop {
        match open() {
            Ok(value) => return Ok(value),
            Err(e) => {
                if is_corrupted(&e) {
                    cleanup();
                }
                if attempt >= max_attempts {
                    return Err(e);
                }
                backoff(attempt);
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum TestErr {
        Corrupted,
        Transient,
    }

    fn is_corrupted(e: &TestErr) -> bool {
        matches!(e, TestErr::Corrupted)
    }

    #[test]
    fn returns_ok_on_first_success_without_cleanup_or_backoff() {
        let mut cleanups = 0;
        let mut backoffs = 0;
        let result: Result<i32, TestErr> = retry_open(
            3,
            || Ok(42),
            is_corrupted,
            || cleanups += 1,
            |_| backoffs += 1,
        );
        assert_eq!(result.unwrap(), 42);
        assert_eq!(cleanups, 0, "no cleanup on a clean first open");
        assert_eq!(backoffs, 0, "no backoff when the first attempt succeeds");
    }

    #[test]
    fn gives_up_after_max_attempts_returning_the_last_error() {
        let mut opens = 0;
        let mut cleanups = 0;
        let mut backoffs = 0;
        let result: Result<i32, TestErr> = retry_open(
            3,
            || {
                opens += 1;
                Err(TestErr::Transient)
            },
            is_corrupted,
            || cleanups += 1,
            |_| backoffs += 1,
        );
        assert_eq!(result.unwrap_err(), TestErr::Transient);
        assert_eq!(opens, 3, "tries exactly max_attempts times");
        assert_eq!(backoffs, 2, "backs off between attempts, not after the last");
        assert_eq!(cleanups, 0, "transient errors are not corruption");
    }

    #[test]
    fn cleans_a_corrupted_service_then_retries_to_success() {
        let mut opens = 0;
        let mut cleanups = 0;
        let mut backoffs = 0;
        let result: Result<i32, TestErr> = retry_open(
            5,
            || {
                opens += 1;
                if opens <= 2 {
                    Err(TestErr::Corrupted)
                } else {
                    Ok(7)
                }
            },
            is_corrupted,
            || cleanups += 1,
            |_| backoffs += 1,
        );
        assert_eq!(result.unwrap(), 7, "recovers once the corruption is cleared");
        assert_eq!(opens, 3);
        assert_eq!(cleanups, 2, "cleans the corrupted service before each retry");
        assert_eq!(backoffs, 2);
    }

    #[test]
    fn does_not_clean_on_a_transient_non_corruption_error() {
        let mut opens = 0;
        let mut cleanups = 0;
        let result: Result<i32, TestErr> = retry_open(
            5,
            || {
                opens += 1;
                if opens == 1 {
                    Err(TestErr::Transient)
                } else {
                    Ok(1)
                }
            },
            is_corrupted,
            || cleanups += 1,
            |_| {},
        );
        assert_eq!(result.unwrap(), 1);
        assert_eq!(cleanups, 0, "no cleanup for a non-corruption error");
    }
}
