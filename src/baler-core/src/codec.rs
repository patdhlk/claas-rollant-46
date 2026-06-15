//! [`PostcardCodec`] — the wire codec for the UI⇄daemon channels (ISSUE_0011).
//!
//! taktora's `transport-iox` channels are typed over an application payload plus
//! a [`PayloadCodec`] that serialises it into the envelope's inline byte buffer.
//! The baler link uses [postcard](https://docs.rs/postcard): a compact,
//! `no_std`-friendly, schema-free binary format — a good fit for the tiny,
//! fixed-shape [`StateSnapshot`](crate::StateSnapshot) /
//! [`Command`](crate::Command) payloads.
//!
//! The codec is pure (no iceoryx2), so its round-trip is unit-tested on the host.

use serde::de::DeserializeOwned;
use serde::Serialize;
use taktora_connector_core::{ConnectorError, PayloadCodec};

/// postcard-backed [`PayloadCodec`] for the baler IPC channels.
#[derive(Clone, Copy, Debug, Default)]
pub struct PostcardCodec;

impl PayloadCodec for PostcardCodec {
    fn format_name(&self) -> &'static str {
        "postcard"
    }

    fn encode<T>(&self, value: &T, buf: &mut [u8]) -> Result<usize, ConnectorError>
    where
        T: Serialize,
    {
        let used =
            postcard::to_slice(value, buf).map_err(|e| ConnectorError::codec("postcard", e))?;
        Ok(used.len())
    }

    fn decode<T>(&self, buf: &[u8]) -> Result<T, ConnectorError>
    where
        T: DeserializeOwned,
    {
        postcard::from_bytes(buf).map_err(|e| ConnectorError::codec("postcard", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{Command, KnifePos, Mode, StateSnapshot};

    fn sample_snapshot() -> StateSnapshot {
        StateSnapshot {
            mode: Mode::Operational,
            bale_full: true,
            knife: KnifePos::Out,
            wrap_armed: true,
            wrap_active: false,
            knife_active: true,
            session: 17,
            total: 123_456,
            ip: [192, 168, 1, 102],
            ip_valid: true,
        }
    }

    #[test]
    fn state_snapshot_round_trips_through_the_wire() {
        let codec = PostcardCodec;
        let original = sample_snapshot();

        let mut buf = [0u8; crate::channel::PAYLOAD_MAX];
        let len = codec.encode(&original, &mut buf).expect("encode");

        let decoded: StateSnapshot = codec.decode(&buf[..len]).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn command_round_trips_through_the_wire() {
        let codec = PostcardCodec;
        let mut buf = [0u8; crate::channel::PAYLOAD_MAX];
        for cmd in [
            Command::Wrap,
            Command::ToggleKnife,
            Command::ResetSession,
            Command::ResetTotal,
            Command::EnterEthernet,
            Command::ReturnToEthercat,
        ] {
            let len = codec.encode(&cmd, &mut buf).expect("encode");
            let decoded: Command = codec.decode(&buf[..len]).expect("decode");
            assert_eq!(decoded, cmd);
        }
    }

    #[test]
    fn encode_into_too_small_a_buffer_errors_rather_than_truncating() {
        let codec = PostcardCodec;
        let mut tiny = [0u8; 1];
        assert!(codec.encode(&sample_snapshot(), &mut tiny).is_err());
    }
}
