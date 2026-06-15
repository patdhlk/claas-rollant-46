//! UI⇄daemon iceoryx2 link over taktora `transport-iox` (ISSUE_0011).
//!
//! Replaces the deleted hand-rolled `baler-ipc` iceoryx2 stack *and* the relay
//! thread that bridged it into the executor. taktora's channel handles are
//! `Send` (unlike the old `baler-ipc` ports, which held an `Rc`), so a
//! [`DaemonLink`] moves straight into the executor's control item — the daemon
//! publishes state and drains commands inline on the control cycle.
//!
//! Two single-direction services, named in [`baler_core::channel`]:
//! * [`CHANNEL_STATE`] — daemon writes [`StateSnapshot`], UI reads.
//! * [`CHANNEL_COMMAND`] — UI writes [`Command`], daemon reads.
//!
//! Both sides `open_or_create`, so they can race at boot and corrupt a service
//! (`ServiceInCorruptedState`). [`DaemonLink::new`] defeats that with
//! [`baler_core::bringup::retry_open`]: on a corruption error it runs iceoryx2's
//! dead-node cleanup (clearing the stale service a half-creating process left
//! behind) and retries with backoff, rather than dying deaf-and-mute.

use std::time::Duration;

use baler_core::bringup::retry_open;
use baler_core::channel::{AppRouting, CHANNEL_COMMAND, CHANNEL_STATE, PAYLOAD_MAX};
use baler_core::codec::PostcardCodec;
use baler_core::{Command, StateSnapshot};
use iceoryx2::node::Node;
use iceoryx2::prelude::{ipc, NodeBuilder};
use taktora_connector_core::{ChannelDescriptor, ConnectorError};
use taktora_connector_transport_iox::{ChannelReader, ChannelWriter, ServiceFactory};

/// Inline payload size for both channels — must match the UI side.
const N: usize = PAYLOAD_MAX;

/// Bring-up attempts before giving up on a channel (covers the boot race).
const OPEN_ATTEMPTS: u32 = 10;

/// Daemon-side handle to the UI link: [`StateSnapshot`] out, [`Command`]s in.
pub struct DaemonLink {
    // Kept alive for the lifetime of the ports (iceoryx2 requires the node to
    // outlive its publishers/subscribers). taktora's handles own their ports and
    // do not borrow the node, so this struct is not self-referential.
    _node: Node<ipc::Service>,
    state_tx: ChannelWriter<StateSnapshot, PostcardCodec, N>,
    cmd_rx: ChannelReader<Command, PostcardCodec, N>,
}

impl DaemonLink {
    /// Build the node and open both channels, recovering from the boot race by
    /// cleaning stale/corrupted services and retrying (ISSUE_0011).
    pub fn new() -> Result<Self, ConnectorError> {
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|e| stack_msg(format!("node create: {e:?}")))?;

        let (state_tx, cmd_rx) = {
            let factory = ServiceFactory::new(&node);
            let state_desc = ChannelDescriptor::<AppRouting, N>::new(CHANNEL_STATE, AppRouting)?;
            let cmd_desc = ChannelDescriptor::<AppRouting, N>::new(CHANNEL_COMMAND, AppRouting)?;

            let state_tx = open_with_recovery(&node, || {
                factory.create_writer::<StateSnapshot, _, _, N>(&state_desc, PostcardCodec)
            })?;
            let cmd_rx = open_with_recovery(&node, || {
                factory.create_reader::<Command, _, _, N>(&cmd_desc, PostcardCodec)
            })?;
            (state_tx, cmd_rx)
        };

        Ok(Self {
            _node: node,
            state_tx,
            cmd_rx,
        })
    }

    /// Publish the latest snapshot. Best-effort: a dropped frame is harmless —
    /// the UI only ever renders the newest one.
    pub fn publish(&self, snapshot: &StateSnapshot) {
        let _ = self.state_tx.send(snapshot);
    }

    /// Drain all pending operator commands, oldest first.
    pub fn drain(&self) -> Vec<Command> {
        let mut out = Vec::new();
        while let Ok(Some(env)) = self.cmd_rx.try_recv() {
            out.push(env.value);
        }
        out
    }
}

/// Open a channel with the ISSUE_0011 race-recovery policy: clean iceoryx2's
/// dead-node resources on a corruption error, back off, and retry.
fn open_with_recovery<T>(
    node: &Node<ipc::Service>,
    open: impl FnMut() -> Result<T, ConnectorError>,
) -> Result<T, ConnectorError> {
    retry_open(
        OPEN_ATTEMPTS,
        open,
        is_corrupted,
        || {
            // Clears resources left by a process that died mid-create — exactly
            // the stale/corrupted `baler.state` the boot race produces.
            Node::<ipc::Service>::cleanup_dead_nodes(node.config());
        },
        |attempt| std::thread::sleep(backoff_for(attempt)),
    )
}

/// True when the failure is iceoryx2's `ServiceInCorruptedState`. taktora wraps
/// the open error in [`ConnectorError::Stack`], whose `Debug` carries the
/// iceoryx2 variant name.
fn is_corrupted(e: &ConnectorError) -> bool {
    format!("{e:?}").contains("Corrupted")
}

/// Linear backoff capped at 500 ms — short enough that the UI catches up within
/// a second once the daemon has created the services.
fn backoff_for(attempt: u32) -> Duration {
    Duration::from_millis((u64::from(attempt) * 50).min(500))
}

/// Wrap a free-form message as a [`ConnectorError::Stack`] (its constructor wants
/// a `std::error::Error`).
fn stack_msg(msg: String) -> ConnectorError {
    #[derive(Debug)]
    struct Msg(String);
    impl core::fmt::Display for Msg {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::error::Error for Msg {}
    ConnectorError::stack(Msg(msg))
}
