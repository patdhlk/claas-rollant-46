//! iceoryx2 0.8 zero-copy IPC transport for the two-process baler controller.
//!
//! Decentralized (ISSUE_0002): no RouDi/daemon. Both processes call
//! [`build_node`], which pins an identical [`iceoryx2::config::Config`]
//! (root-path + prefix) so the daemon and the UI land in the same shared-memory
//! namespace, then open the same two services by name:
//!
//! * [`SERVICE_STATE`] — daemon publishes [`StateSnapshot`], UI subscribes.
//! * [`SERVICE_COMMAND`] — UI publishes [`Command`], daemon subscribes.
//!
//! Both sides use `ipc::Service` and `open_or_create()`, so whichever process
//! starts first creates the service and the other joins it.
//!
//! This whole module is gated behind the `hardware` cargo feature; with the
//! feature off (the macOS host build) iceoryx2 is not a dependency at all.

use core::time::Duration;

use iceoryx2::config::Config;
use iceoryx2::node::NodeBuilder;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::{FileName, Path, ZeroCopySend};

// Re-export so callers (e.g. baler-ui's IpcBackend) can name the node type.
pub use iceoryx2::node::Node;
pub use iceoryx2::service::ipc::Service;

use crate::{Command, StateSnapshot};

/// Service carrying [`StateSnapshot`] from the daemon to the UI.
pub const SERVICE_STATE: &str = "baler/state";

/// Service carrying [`Command`] from the UI to the daemon.
pub const SERVICE_COMMAND: &str = "baler/command";

/// Shared-memory root path. Both processes MUST agree on this (combined with
/// [`SHARED_PREFIX`]) to land in the same namespace.
const ROOT_PATH: &[u8] = b"/tmp/baler_ipc/";

/// File prefix for all iceoryx2 runtime files. Both processes MUST agree.
const SHARED_PREFIX: &[u8] = b"baler_";

/// How many state samples the daemon keeps in service history so a
/// late-joining UI immediately gets the most recent frame.
const STATE_HISTORY: usize = 1;

/// Subscriber buffer for the UI: 1 is enough — the UI only wants the latest
/// frame each render tick and drains the queue every poll.
const STATE_SUBSCRIBER_BUFFER: usize = 4;

/// Subscriber buffer for the daemon command port: hold a small burst of
/// operator commands between 10 ms scan ticks without dropping any.
const COMMAND_SUBSCRIBER_BUFFER: usize = 16;

/// Errors raised while constructing the transport. Steady-state send/receive
/// errors are surfaced as their own variants but never panic.
#[derive(Debug)]
pub enum TransportError {
    /// The shared [`Config`] (root-path / prefix) could not be built.
    Config(String),
    /// The iceoryx2 [`Node`] could not be created.
    Node(String),
    /// A service name was rejected, or a service / port could not be
    /// opened or created.
    Service(String),
    /// A publish (loan or send) failed.
    Send(String),
    /// A receive failed (distinct from the empty-queue `Ok(None)` case).
    Receive(String),
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TransportError::Config(m) => write!(f, "ipc config error: {m}"),
            TransportError::Node(m) => write!(f, "ipc node error: {m}"),
            TransportError::Service(m) => write!(f, "ipc service error: {m}"),
            TransportError::Send(m) => write!(f, "ipc send error: {m}"),
            TransportError::Receive(m) => write!(f, "ipc receive error: {m}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Result alias for transport operations.
pub type Result<T> = core::result::Result<T, TransportError>;

/// Build the shared [`Config`]: identical root-path and prefix on both sides.
fn shared_config() -> Result<Config> {
    let mut config = Config::default();

    // `prefix` is a public `FileName` field; build it via the verified
    // `TryFrom<&str>` impl.
    let prefix_str = core::str::from_utf8(SHARED_PREFIX)
        .map_err(|e| TransportError::Config(format!("prefix not utf8: {e}")))?;
    config.global.prefix = FileName::try_from(prefix_str)
        .map_err(|e| TransportError::Config(format!("bad prefix: {e:?}")))?;

    // `root_path` is a setter taking `&Path`; build a normalized Path.
    let root = Path::new_normalized(ROOT_PATH)
        .map_err(|e| TransportError::Config(format!("bad root path: {e:?}")))?;
    config.global.set_root_path(&root);

    Ok(config)
}

/// Build the iceoryx2 [`Node`] both ports hang off, with the shared config so
/// the daemon and UI share one namespace. Call once per process.
pub fn build_node() -> Result<Node<Service>> {
    let config = shared_config()?;
    NodeBuilder::new()
        .config(&config)
        .create::<Service>()
        .map_err(|e| TransportError::Node(format!("{e:?}")))
}

/// Open or create a publish-subscribe service for payload `T` by name, with the
/// given history and subscriber buffer, returning the port factory.
fn open_service<T: ZeroCopySend + core::fmt::Debug + 'static>(
    node: &Node<Service>,
    name: &str,
    history_size: usize,
    subscriber_buffer: usize,
) -> Result<
    iceoryx2::service::port_factory::publish_subscribe::PortFactory<Service, T, ()>,
> {
    let service_name = name
        .try_into()
        .map_err(|e| TransportError::Service(format!("bad service name {name:?}: {e:?}")))?;

    node.service_builder(&service_name)
        .publish_subscribe::<T>()
        .max_publishers(1)
        .max_subscribers(1)
        .history_size(history_size)
        .subscriber_max_buffer_size(subscriber_buffer)
        .open_or_create()
        .map_err(|e| TransportError::Service(format!("open service {name:?}: {e:?}")))
}

// ---------------------------------------------------------------------------
// Daemon side
// ---------------------------------------------------------------------------

/// Daemon-side publisher of [`StateSnapshot`]. One per process.
pub struct StatePublisher {
    publisher: Publisher<Service, StateSnapshot, ()>,
}

impl StatePublisher {
    /// Open (or create) the state service and a publisher on it.
    pub fn new(node: &Node<Service>) -> Result<Self> {
        let service =
            open_service::<StateSnapshot>(node, SERVICE_STATE, STATE_HISTORY, STATE_SUBSCRIBER_BUFFER)?;
        let publisher = service
            .publisher_builder()
            .create()
            .map_err(|e| TransportError::Service(format!("state publisher: {e:?}")))?;
        Ok(Self { publisher })
    }

    /// Publish one snapshot, zero-copy. Loan -> write -> send, no panics.
    pub fn publish(&self, snapshot: &StateSnapshot) -> Result<()> {
        let sample = self
            .publisher
            .loan_uninit()
            .map_err(|e| TransportError::Send(format!("loan state: {e:?}")))?;
        let sample = sample.write_payload(*snapshot);
        sample
            .send()
            .map_err(|e| TransportError::Send(format!("send state: {e:?}")))?;
        Ok(())
    }
}

/// Daemon-side receiver of [`Command`]s from the UI. Non-blocking; drained
/// each 10 ms scan tick.
pub struct CommandReceiver {
    subscriber: Subscriber<Service, Command, ()>,
}

impl CommandReceiver {
    /// Open (or create) the command service and a subscriber on it.
    pub fn new(node: &Node<Service>) -> Result<Self> {
        let service = open_service::<Command>(
            node,
            SERVICE_COMMAND,
            // History is irrelevant on the consumer side; keep it minimal.
            1,
            COMMAND_SUBSCRIBER_BUFFER,
        )?;
        let subscriber = service
            .subscriber_builder()
            .create()
            .map_err(|e| TransportError::Service(format!("command subscriber: {e:?}")))?;
        Ok(Self { subscriber })
    }

    /// Drain all pending commands without blocking. Returns oldest-first.
    /// `receive()` returns `Ok(None)` immediately when the queue is empty, so
    /// this loop never blocks the scan cycle.
    pub fn drain(&self) -> Result<Vec<Command>> {
        let mut out = Vec::new();
        while let Some(sample) = self
            .subscriber
            .receive()
            .map_err(|e| TransportError::Receive(format!("recv command: {e:?}")))?
        {
            out.push(*sample);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// UI side
// ---------------------------------------------------------------------------

/// UI-side poller for the latest [`StateSnapshot`]. Non-blocking; called in the
/// render loop.
pub struct StatePoller {
    subscriber: Subscriber<Service, StateSnapshot, ()>,
}

impl StatePoller {
    /// Open (or create) the state service and a subscriber on it.
    pub fn new(node: &Node<Service>) -> Result<Self> {
        let service =
            open_service::<StateSnapshot>(node, SERVICE_STATE, STATE_HISTORY, STATE_SUBSCRIBER_BUFFER)?;
        let subscriber = service
            .subscriber_builder()
            .create()
            .map_err(|e| TransportError::Service(format!("state subscriber: {e:?}")))?;
        Ok(Self { subscriber })
    }

    /// Return the most recent snapshot, discarding any older queued samples,
    /// or `Ok(None)` if none has arrived yet. Non-blocking — `receive()`
    /// returns `Ok(None)` on an empty queue, so the render loop never stalls.
    pub fn latest(&self) -> Result<Option<StateSnapshot>> {
        let mut latest = None;
        while let Some(sample) = self
            .subscriber
            .receive()
            .map_err(|e| TransportError::Receive(format!("recv state: {e:?}")))?
        {
            latest = Some(*sample);
        }
        Ok(latest)
    }
}

/// UI-side sender of a single [`Command`] to the daemon.
pub struct CommandSender {
    publisher: Publisher<Service, Command, ()>,
}

impl CommandSender {
    /// Open (or create) the command service and a publisher on it.
    pub fn new(node: &Node<Service>) -> Result<Self> {
        let service =
            open_service::<Command>(node, SERVICE_COMMAND, 1, COMMAND_SUBSCRIBER_BUFFER)?;
        let publisher = service
            .publisher_builder()
            .create()
            .map_err(|e| TransportError::Service(format!("command publisher: {e:?}")))?;
        Ok(Self { publisher })
    }

    /// Send one command, zero-copy. Loan -> write -> send.
    pub fn send(&self, command: Command) -> Result<()> {
        let sample = self
            .publisher
            .loan_uninit()
            .map_err(|e| TransportError::Send(format!("loan command: {e:?}")))?;
        let sample = sample.write_payload(command);
        sample
            .send()
            .map_err(|e| TransportError::Send(format!("send command: {e:?}")))?;
        Ok(())
    }
}

/// Convenience cycle time hint for callers that drive a node-based wait loop.
/// The transport itself never blocks; this is only exported so the daemon and
/// UI can share one constant if they choose to use `node.wait(_)`.
pub const SCAN_CYCLE: Duration = Duration::from_millis(10);
