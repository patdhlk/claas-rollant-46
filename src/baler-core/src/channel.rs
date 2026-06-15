//! Channel contract for the UI⇄daemon iceoryx2 link (ISSUE_0011).
//!
//! Both processes name the same two taktora `transport-iox` pub/sub services and
//! agree on the inline payload size. The actual `ServiceFactory` / `Node` wiring
//! lives in the binaries (it pulls iceoryx2, gated behind the `hardware` feature);
//! only the names, the payload size, and the marker [`AppRouting`] are pure
//! enough to live here in the host-built contract.

use taktora_connector_core::Routing;

/// Marker [`Routing`] for app↔app channels. The transport layer only uses a
/// descriptor's *name* to derive the iceoryx2 service; routing carries no
/// information for a process-to-process link (unlike the EtherCAT connector,
/// whose routing maps PDO bit slices), so this is an empty marker.
#[derive(Clone, Debug)]
pub struct AppRouting;

impl Routing for AppRouting {}

/// Service carrying [`crate::StateSnapshot`] from the daemon to the UI.
pub const CHANNEL_STATE: &str = "baler.state";

/// Service carrying [`crate::Command`] from the UI to the daemon.
pub const CHANNEL_COMMAND: &str = "baler.command";

/// Inline envelope payload size (`N`) for both channels. `StateSnapshot` and
/// `Command` postcard-encode to a few dozen bytes; 256 is generous headroom and
/// keeps each channel's shared-memory footprint trivial.
pub const PAYLOAD_MAX: usize = 256;
