//! Network-mode switch for the single Ethernet NIC shared between EtherCAT and IP.
//!
//! Design decision: shell out to iproute2 `ip` (already on the Yocto image) via
//! `std::process::Command` rather than the `rtnetlink` crate — `rtnetlink` pulls
//! in tokio, and the daemon (taktora) is a cyclic, non-async scheduler; adding an
//! async runtime for three one-shot commands is not justified. Zero extra deps.
//!
//! The EtherCAT master (ethercrab, EtherCatIo layer) is stopped before
//! `enter_ethernet` and recreated after `enter_ethercat`; this module never
//! touches the master — it only manages L3 (IP addresses + link state) on a
//! named interface.

use std::net::Ipv4Addr;

use crate::ports::{NetworkController, NetworkError};

/// Configuration + executor for switching `iface` between EtherCAT and Ethernet.
#[derive(Debug, Clone)]
pub struct NetworkMode {
    iface: String,
    ip: Ipv4Addr,
    prefix: u8,
    gateway: Option<Ipv4Addr>,
}

impl NetworkMode {
    /// Construct with the interface name (e.g. `"eth0"`), the static IPv4 to
    /// assign in Ethernet mode, and the network prefix length (CIDR, clamped to 32).
    pub fn new(iface: impl Into<String>, ip: Ipv4Addr, prefix: u8) -> Self {
        Self {
            iface: iface.into(),
            ip,
            prefix: prefix.min(32),
            gateway: None,
        }
    }

    /// Set an optional default gateway. Builder-style.
    #[allow(dead_code)]
    pub fn with_gateway(mut self, gateway: Ipv4Addr) -> Self {
        self.gateway = Some(gateway);
        self
    }

    /// The interface this controller manages.
    #[allow(dead_code)]
    pub fn iface(&self) -> &str {
        &self.iface
    }

    /// The static address in `a.b.c.d/prefix` form.
    fn cidr(&self) -> String {
        format!("{}/{}", self.ip, self.prefix)
    }
}

#[cfg(feature = "netmode-hw")]
mod imp {
    use super::*;
    use std::process::Command;

    /// Run `ip <args...>`, mapping spawn failure and non-zero exit to
    /// `NetworkError` with full stderr capture.
    fn run_ip(args: &[&str]) -> Result<(), NetworkError> {
        let argv = format!("ip {}", args.join(" "));
        let output = Command::new("ip")
            .args(args)
            .output()
            .map_err(|source| NetworkError::Spawn {
                argv: argv.clone(),
                source,
            })?;

        if output.status.success() {
            return Ok(());
        }

        Err(NetworkError::Command {
            argv,
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    impl NetworkController for NetworkMode {
        /// Ethernet mode: link up, flush stale addresses, assign the static IP,
        /// optionally install the default route. Returns the assigned address.
        fn enter_ethernet(&mut self) -> Result<Ipv4Addr, NetworkError> {
            let iface = self.iface.clone();
            let cidr = self.cidr();

            run_ip(&["link", "set", "dev", &iface, "up"])?;
            run_ip(&["addr", "flush", "dev", &iface])?;
            run_ip(&["addr", "add", &cidr, "dev", &iface])?;

            if let Some(gw) = self.gateway {
                let gw = gw.to_string();
                run_ip(&["route", "replace", "default", "via", &gw, "dev", &iface])?;
            }

            Ok(self.ip)
        }

        /// EtherCAT mode: remove the default route (if any) and flush all IP
        /// addresses so the NIC carries no L3 config; the link is left up so
        /// ethercrab can open its raw `AF_PACKET` socket and drive the bus.
        fn enter_ethercat(&mut self) -> Result<(), NetworkError> {
            let iface = self.iface.clone();

            if self.gateway.is_some() {
                if let Err(e) = run_ip(&["route", "del", "default", "dev", &iface]) {
                    if !is_missing_route(&e) {
                        return Err(e);
                    }
                }
            }

            run_ip(&["addr", "flush", "dev", &iface])?;
            run_ip(&["link", "set", "dev", &iface, "up"])?;
            Ok(())
        }
    }

    /// True if the error is iproute2's "route does not exist", benign when
    /// tearing down a route that was never installed.
    fn is_missing_route(e: &NetworkError) -> bool {
        match e {
            NetworkError::Command { stderr, .. } => {
                let s = stderr.to_ascii_lowercase();
                s.contains("no such process") || s.contains("cannot find")
            }
            NetworkError::Spawn { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_formats_address_and_prefix() {
        let nm = NetworkMode::new("eth0", Ipv4Addr::new(192, 168, 1, 50), 24);
        assert_eq!(nm.cidr(), "192.168.1.50/24");
        assert_eq!(nm.iface(), "eth0");
    }

    #[test]
    fn prefix_is_clamped() {
        let nm = NetworkMode::new("eth0", Ipv4Addr::new(10, 0, 0, 1), 99);
        assert_eq!(nm.cidr(), "10.0.0.1/32");
    }

    #[test]
    fn gateway_is_optional() {
        let nm = NetworkMode::new("eth0", Ipv4Addr::LOCALHOST, 8);
        assert!(nm.gateway.is_none());
        let nm = nm.with_gateway(Ipv4Addr::new(10, 0, 0, 254));
        assert_eq!(nm.gateway, Some(Ipv4Addr::new(10, 0, 0, 254)));
    }
}
