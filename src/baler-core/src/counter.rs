//! Session/total bale counters with atomic persistence (REQ_0007).
//!
//! Counts survive power loss: every change is written to a temp file in the same
//! directory, fsync'd, then `rename`d over the target (an atomic replace on
//! POSIX). Counters reload on construction. The session counter resets only on
//! explicit operator action; the total reset is reserved for the PIN-gated
//! service screen (the UI enforces the PIN before sending the command).

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    pub session: u64,
    pub total: u64,
}

#[derive(Debug)]
pub struct CounterStore {
    path: PathBuf,
    counters: Counters,
}

impl CounterStore {
    /// Load from `path`, starting from zero if the file does not yet exist.
    /// A malformed file is treated as zero rather than failing the daemon.
    pub fn load(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let counters = match fs::read_to_string(&path) {
            Ok(s) => parse(&s).unwrap_or_default(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Counters::default(),
            Err(e) => return Err(e),
        };
        Ok(Self { path, counters })
    }

    pub fn snapshot(&self) -> Counters {
        self.counters
    }

    /// Increment both counters by one (a clean wrap completion, REQ_0004).
    pub fn increment_wrap(&mut self) -> io::Result<()> {
        self.counters.session = self.counters.session.saturating_add(1);
        self.counters.total = self.counters.total.saturating_add(1);
        self.persist()
    }

    pub fn reset_session(&mut self) -> io::Result<()> {
        self.counters.session = 0;
        self.persist()
    }

    pub fn reset_total(&mut self) -> io::Result<()> {
        self.counters.total = 0;
        self.persist()
    }

    fn persist(&self) -> io::Result<()> {
        let dir = self.path.parent().filter(|p| !p.as_os_str().is_empty());
        if let Some(dir) = dir {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            write!(
                f,
                "session={}\ntotal={}\n",
                self.counters.session, self.counters.total
            )?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &self.path)
    }
}

fn parse(s: &str) -> Option<Counters> {
    let mut c = Counters::default();
    for line in s.lines() {
        let (k, v) = line.split_once('=')?;
        let v: u64 = v.trim().parse().ok()?;
        match k.trim() {
            "session" => c.session = v,
            "total" => c.total = v,
            _ => {}
        }
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch path per test, with no dependency on time/randomness.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("baler-counter-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("counters")
    }

    #[test]
    fn missing_file_loads_as_zero() {
        let p = scratch("missing");
        let c = CounterStore::load(&p).unwrap();
        assert_eq!(c.snapshot(), Counters { session: 0, total: 0 });
    }

    #[test]
    fn increment_persists_and_reloads() {
        let p = scratch("increment");
        let mut c = CounterStore::load(&p).unwrap();
        c.increment_wrap().unwrap();
        c.increment_wrap().unwrap();
        assert_eq!(c.snapshot(), Counters { session: 2, total: 2 });

        // A fresh load (e.g. after a power cycle) sees the persisted values.
        let reloaded = CounterStore::load(&p).unwrap();
        assert_eq!(reloaded.snapshot(), Counters { session: 2, total: 2 });
    }

    #[test]
    fn session_reset_keeps_total() {
        let p = scratch("session-reset");
        let mut c = CounterStore::load(&p).unwrap();
        for _ in 0..5 {
            c.increment_wrap().unwrap();
        }
        c.reset_session().unwrap();
        assert_eq!(c.snapshot(), Counters { session: 0, total: 5 });
        let reloaded = CounterStore::load(&p).unwrap();
        assert_eq!(reloaded.snapshot(), Counters { session: 0, total: 5 });
    }

    #[test]
    fn total_reset_is_independent() {
        let p = scratch("total-reset");
        let mut c = CounterStore::load(&p).unwrap();
        c.increment_wrap().unwrap();
        c.reset_total().unwrap();
        assert_eq!(c.snapshot(), Counters { session: 1, total: 0 });
    }

    #[test]
    fn malformed_file_is_treated_as_zero() {
        let p = scratch("malformed");
        fs::write(&p, "garbage not parseable\n").unwrap();
        let c = CounterStore::load(&p).unwrap();
        assert_eq!(c.snapshot(), Counters::default());
    }
}
