//! Linux hardware-watchdog petter for the ifm CR1140 (Yocto Linux), REQ_0010.
//!
//! Behind the `hardware` cargo feature (gated at the `mod` site in main.rs).
//!
//! Lifecycle: opening `/dev/watchdog` ARMS the watchdog; each healthy scan
//! cycle calls [`Watchdog::pet`] (a keepalive). If the loop stalls, keepalives
//! stop and the SoC resets the device. On `Drop`, the magic-close byte `'V'`
//! disarms it *iff* `magic_close == true` (default `false`: a safety daemon
//! that exits should still trigger a reset).

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;

use crate::ports::Watchdog;

// WDIOC_SETTIMEOUT = _IOWR('W', 6, int). `ioctl_readwrite!` produces the _IOWR
// direction, so the encoded request number is byte-for-byte WDIOC_SETTIMEOUT;
// the driver copies the granted timeout back into the int.
nix::ioctl_readwrite!(wdioc_settimeout, b'W', 6, libc::c_int);

/// Keeps the Linux hardware watchdog alive while the daemon's scan loop is healthy.
pub struct WatchdogPetter {
    dev: File,
    magic_close: bool,
}

impl WatchdogPetter {
    /// Open the watchdog device, arming the hardware watchdog.
    ///
    /// * `path` — usually `"/dev/watchdog"`.
    /// * `timeout_secs` — if `Some`, request via `WDIOC_SETTIMEOUT` (the kernel
    ///   may clamp/round; a differing granted value is not an error).
    /// * `magic_close` — `false` (default) keeps the watchdog armed on exit.
    pub fn open(path: &str, timeout_secs: Option<u32>, magic_close: bool) -> io::Result<Self> {
        let dev = OpenOptions::new().write(true).open(path)?;

        if let Some(secs) = timeout_secs {
            // SAFETY: `dev` owns a valid open fd for the call; `&mut t` is a live
            // `c_int`. The driver reads the requested timeout and writes the
            // granted value back into `t`.
            let mut t: libc::c_int = secs as libc::c_int;
            let fd = dev.as_raw_fd();
            unsafe { wdioc_settimeout(fd, &mut t) }
                .map_err(|errno| io::Error::from_raw_os_error(errno as i32))?;
        }

        Ok(Self { dev, magic_close })
    }
}

impl Watchdog for WatchdogPetter {
    /// Issue a keepalive by writing a single non-`'V'` byte. Infallible per the
    /// trait: a write failure is logged and swallowed — a broken fd simply lets
    /// the hardware time out and reset (the fail-safe we want).
    fn pet(&mut self) {
        if let Err(e) = self.dev.write_all(b"\0") {
            log::warn!("watchdog keepalive write failed: {e}");
        }
    }
}

impl Drop for WatchdogPetter {
    fn drop(&mut self) {
        if self.magic_close {
            if let Err(e) = self.dev.write_all(b"V") {
                log::warn!("watchdog magic-close write failed: {e}");
            }
        }
        // File's Drop closes the fd. With magic_close == false we intentionally
        // leave the watchdog armed.
    }
}
