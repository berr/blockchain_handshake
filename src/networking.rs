use crate::message::{Message, MessageKind};
use anyhow::{bail, Result};
use byteorder::{BigEndian, LittleEndian, WriteBytesExt};
use std::io::Write;
use std::iter;
use tokio::io::{AsyncRead, AsyncWrite};

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Chain {
    Main = 0xf9beb4d9,
    TestNet = 0x0b110907,
    RegTest = 0xfabfb5da,
}

impl TryFrom<u32> for Chain {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self> {
        let e = match value {
            x if x == Chain::Main as u32 => Chain::Main,
            x if x == Chain::TestNet as u32 => Chain::TestNet,
            x if x == Chain::RegTest as u32 => Chain::RegTest,
            _ => bail!("Invalid chain id: {value:#010X?}"),
        };

        Ok(e)
    }
}

/// Serialize message, add a header and send it through a Sender
///
/// Since we generated the message, there isn't anything to be defensive here.
pub async fn send_message<W: AsyncWrite + Unpin + Send, M: Into<Message>>(
    writer: &mut W,
    chain: Chain,
    message: M,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let message = message.into();
    const HEADER_SIZE: usize = 24;
    let mut whole_message = vec![0u8; HEADER_SIZE];

    message.serialize(&mut whole_message)?;

    let payload = &whole_message[HEADER_SIZE..];
    let payload_len = payload.len();
    let checksum = blockchain_checksum(payload);

    let mut chain_buffer = &mut whole_message[0..4];
    WriteBytesExt::write_u32::<BigEndian>(&mut chain_buffer, chain as u32)?;

    let mut command_buffer = &mut whole_message[4..16];
    Write::write_all(&mut command_buffer, message.kind().command())?;

    let mut payload_buffer = &mut whole_message[16..20];
    WriteBytesExt::write_u32::<LittleEndian>(&mut payload_buffer, payload_len as u32)?;

    let mut checksum_buffer = &mut whole_message[20..24];
    WriteBytesExt::write_u32::<BigEndian>(&mut checksum_buffer, checksum)?;

    AsyncWriteExt::write_all(writer, &whole_message).await?;
    AsyncWriteExt::flush(writer).await?;
    Ok(())
}

/// Receive a message for the given chain
///
/// Can fail in multiple ways:
/// - Invalid or wrong Chain in header
/// - Unknown command
/// - Payload is too big
/// - Payload has an unexpected size (for messages with fixed length)
/// - Checksum failed
/// - Deserialization failed
pub async fn receive_message<R: AsyncRead + Unpin + Send>(
    chain: Chain,
    reader: &mut R,
) -> Result<Message> {
    use tokio::io::AsyncReadExt;

    let mut header = [0u8; 24];
    reader.read_exact(&mut header).await?;
    let received_chain: Chain = u32::from_be_bytes(header[0..4].try_into().unwrap()).try_into()?;

    if received_chain != chain {
        bail!("Received message for the wrong chain. Expected {chain:?}, Received {received_chain:?}.");
    }

    let mut command = [0u8; 12];
    command.copy_from_slice(&header[4..16]);
    let message_kind: MessageKind = command.try_into()?;

    let payload_size = u32::from_le_bytes(header[16..20].try_into().unwrap());

    let expected_payload_size = message_kind.maximum_allowed_size();
    if !expected_payload_size.accepts(payload_size) {
        bail!("Invalid payload size: Expected {expected_payload_size:?}. Received {payload_size}");
    }

    let received_checksum = u32::from_be_bytes(header[20..24].try_into().unwrap());

    let mut payload: Vec<u8> = iter::repeat(0).take(payload_size as usize).collect();
    reader.read_exact(payload.as_mut()).await?;

    let calculated_checksum = blockchain_checksum(&payload);

    if received_checksum != calculated_checksum {
        bail!("Received invalid checksum. Received {received_checksum:#010X?}, Expected {calculated_checksum:#010X?}");
    }

    let deserialized = Message::deserialize(message_kind, payload)?;

    Ok(deserialized)
}

fn blockchain_checksum(data: &[u8]) -> u32 {
    use sha2::{Digest, Sha256};
    let mut first_hasher = Sha256::new();
    first_hasher.update(data);
    let first_round: [u8; 32] = first_hasher.finalize().into();

    let mut second_hasher = Sha256::new();
    second_hasher.update(&first_round);
    let second_round: [u8; 32] = second_hasher.finalize().into();

    let mut first_bytes = [0u8; 4];
    first_bytes.copy_from_slice(&second_round[..4]);

    u32::from_be_bytes(first_bytes)
}

mod commands {
    pub const VERSION: [u8; 12] = [b'v', b'e', b'r', b's', b'i', b'o', b'n', 0, 0, 0, 0, 0];
    pub const VERACK: [u8; 12] = [b'v', b'e', b'r', b'a', b'c', b'k', 0, 0, 0, 0, 0, 0];
    pub const PING: [u8; 12] = [b'p', b'i', b'n', b'g', 0, 0, 0, 0, 0, 0, 0, 0];
    pub const PONG: [u8; 12] = [b'p', b'o', b'n', b'g', 0, 0, 0, 0, 0, 0, 0, 0];
    pub const SENDCMPCT: [u8; 12] = [
        b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0,
    ];
    pub const WTXIDRELAY: [u8; 12] = [
        b'w', b't', b'x', b'i', b'd', b'r', b'e', b'l', b'a', b'y', 0, 0,
    ];
    pub const GETHEADERS: [u8; 12] = [
        b'g', b'e', b't', b'h', b'e', b'a', b'd', b'e', b'r', b's', 0, 0,
    ];
    pub const FEEFILTER: [u8; 12] = [
        b'f', b'e', b'e', b'f', b'i', b'l', b't', b'e', b'r', 0, 0, 0,
    ];
    pub const SENDADDRV2: [u8; 12] = [
        b's', b'e', b'n', b'd', b'a', b'd', b'd', b'r', b'v', b'2', 0, 0,
    ];
}

trait Command {
    fn command(&self) -> &[u8; 12];
}

impl Command for MessageKind {
    fn command(&self) -> &[u8; 12] {
        match self {
            MessageKind::Version => &commands::VERSION,
            MessageKind::VersionAck => &commands::VERACK,
            MessageKind::Ping => &commands::PING,
            MessageKind::Pong => &commands::PONG,
            MessageKind::SendCompact => &commands::SENDCMPCT,
            MessageKind::GetHeaders => &commands::GETHEADERS,
            MessageKind::FeeFilter => &commands::FEEFILTER,
            MessageKind::WTxIdRelay => &commands::WTXIDRELAY,
            MessageKind::SendAddrV2 => &commands::SENDADDRV2,
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum MaximumPayloadSize {
    Exact(u32),
    AtMost(u32),
}

trait PayloadMaxSize {
    fn maximum_allowed_size(&self) -> MaximumPayloadSize;
}

impl PayloadMaxSize for MessageKind {
    fn maximum_allowed_size(&self) -> MaximumPayloadSize {
        use MaximumPayloadSize::*;

        match self {
            MessageKind::Version => AtMost(512),
            MessageKind::VersionAck => Exact(0),
            MessageKind::Ping => Exact(8),
            MessageKind::Pong => Exact(8),
            MessageKind::SendCompact => Exact(9),
            MessageKind::GetHeaders => AtMost(2048),
            MessageKind::FeeFilter => Exact(8),
            MessageKind::WTxIdRelay => Exact(0),
            MessageKind::SendAddrV2 => Exact(0),
        }
    }
}

impl MaximumPayloadSize {
    pub fn accepts(&self, size: u32) -> bool {
        match self {
            MaximumPayloadSize::Exact(expected) => size == *expected,
            MaximumPayloadSize::AtMost(upper_limit) => size <= *upper_limit,
        }
    }
}

impl TryFrom<[u8; 12]> for MessageKind {
    type Error = anyhow::Error;

    fn try_from(command: [u8; 12]) -> Result<Self> {
        let kind = match &command {
            &commands::VERSION => MessageKind::Version,
            &commands::VERACK => MessageKind::VersionAck,
            &commands::PING => MessageKind::Ping,
            &commands::PONG => MessageKind::Pong,
            &commands::SENDCMPCT => MessageKind::SendCompact,
            &commands::GETHEADERS => MessageKind::GetHeaders,
            &commands::FEEFILTER => MessageKind::FeeFilter,
            &commands::WTXIDRELAY => MessageKind::WTxIdRelay,
            &commands::SENDADDRV2 => MessageKind::SendAddrV2,
            _ => {
                if let Ok(s) = String::from_utf8(command.to_vec()) {
                    bail!("Unknown command: {s}")
                }
                bail!("Unknown command: {command:X?}");
            }
        };

        Ok(kind)
    }
}

#[cfg(test)]
mod test {
    use crate::message::{Message, SendCompactMessage};
    use crate::networking::{
        blockchain_checksum, receive_message, send_message, Chain, MaximumPayloadSize,
    };
    use crate::testing::decode_bytes;
    use std::io::Cursor;

    #[test]
    fn test_checksum_empty() {
        assert_eq!(blockchain_checksum(&[]), 0x5DF6E0E2);
    }

    #[test]
    fn test_checksum_zero() {
        assert_eq!(blockchain_checksum(&[0]), 0x1406e058);
    }

    #[test]
    fn test_checksum_compact_message() {
        let data = [0, 2, 0, 0, 0, 0, 0, 0, 0];
        let calculated = blockchain_checksum(&data);
        assert_eq!(calculated, 0xe92f5ef8);
    }

    #[test]
    fn test_checksum_example() {
        let data: Vec<u8> = vec![
            0x62, 0xEA, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0xB2,
            0xD0, 0x50, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x3B, 0x2E, 0xB3, 0x5D, 0x8C, 0xE6, 0x17, 0x65, 0x0F, 0x2F, 0x53, 0x61,
            0x74, 0x6F, 0x73, 0x68, 0x69, 0x3A, 0x30, 0x2E, 0x37, 0x2E, 0x32, 0x2F, 0xC0, 0x3E,
            0x03, 0x00,
        ];

        let calculated_checksum = blockchain_checksum(&data);
        let expected_checksum = 0x3b648d5au32;
        assert_eq!(
            calculated_checksum, expected_checksum,
            "{calculated_checksum:X?} != {expected_checksum:X?}"
        );
    }

    #[test]
    fn test_checksum_from_execution() {
        let header = decode_bytes("+b602XZlcnNpb24AAAAAAGYAAACx39uS");
        let loaded_checksum = u32::from_be_bytes(header[20..24].try_into().unwrap());

        let data =
            decode_bytes("gBEBAAkEAAAAAAAAJGkPZQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAkEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAogitpThFWBwQL1NhdG9zaGk6MjUuMC4wLwAAAAAB");

        let calculated_checksum = blockchain_checksum(&data);
        assert_eq!(
            calculated_checksum, loaded_checksum,
            "{calculated_checksum:X?} != {loaded_checksum:X?}"
        );
    }

    #[test]
    fn test_maximum_payload_size_exact_accepts() {
        assert!(MaximumPayloadSize::Exact(3).accepts(3));
    }

    #[test]
    fn test_maximum_payload_size_exact_rejects_below() {
        assert!(!MaximumPayloadSize::Exact(3).accepts(2));
    }

    #[test]
    fn test_maximum_payload_size_exact_rejects_above() {
        assert!(!MaximumPayloadSize::Exact(3).accepts(4));
    }

    #[test]
    fn test_maximum_payload_size_upper_limit_accepts_below() {
        assert!(MaximumPayloadSize::AtMost(10).accepts(4));
    }

    #[test]
    fn test_maximum_payload_size_upper_limit_accepts_exact() {
        assert!(MaximumPayloadSize::AtMost(10).accepts(10));
    }

    #[test]
    fn test_maximum_payload_size_upper_limit_rejects_above() {
        assert!(!MaximumPayloadSize::AtMost(10).accepts(32));
    }

    #[tokio::test]
    async fn test_send_message() {
        let data = SendCompactMessage {
            announce: false,
            version: 2,
        };

        let mut output = vec![];
        send_message(&mut output, Chain::Main, data).await.unwrap();

        let expected_output: Vec<u8> = vec![
            0xf9, 0xbe, 0xb4, 0xd9, // main chain
            b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0, // command
            0x09, 0, 0, 0, // payload size
            0xe9, 0x2f, 0x5e, 0xf8, // checksum
            0, 2, 0, 0, 0, 0, 0, 0, 0, // payload
        ];

        assert_eq!(output, expected_output);
    }

    #[tokio::test]
    async fn test_receive_message() {
        let send_compact_message: Vec<u8> = vec![
            0xf9, 0xbe, 0xb4, 0xd9, // main chain
            b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0, // command
            0x09, 0, 0, 0, // payload size
            0xe9, 0x2f, 0x5e, 0xf8, // checksum
            0, 2, 0, 0, 0, 0, 0, 0, 0, // payload
        ];

        let received = receive_message(Chain::Main, &mut Cursor::new(&send_compact_message))
            .await
            .unwrap();

        let Message::SendCompact(message) = received else {
            panic!("Deserialized to wrong type");
        };

        assert_eq!(message.announce, false);
        assert_eq!(message.version, 2);
    }

    #[tokio::test]
    async fn test_receive_message_invalid_chain() {
        let send_compact_message: Vec<u8> = vec![
            0x89, 0xAB, 0xCD, 0xEF, // invalid chain
            b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0, // command
            0x09, 0, 0, 0, // payload size
            0xe9, 0x2f, 0x5e, 0xf8, // checksum
            0, 2, 0, 0, 0, 0, 0, 0, 0, // payload
        ];

        let error_message = receive_message(Chain::Main, &mut Cursor::new(&send_compact_message))
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Invalid chain id: 0x89ABCDEF"),
            "Unexpected error message: {error_message}"
        );
    }

    #[tokio::test]
    async fn test_receive_message_wrong_chain() {
        let send_compact_message: Vec<u8> = vec![
            0xFA, 0xBF, 0xB5, 0xDA, // RegTest
            b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0, // command
            0x09, 0, 0, 0, // payload size
            0xe9, 0x2f, 0x5e, 0xf8, // checksum
            0, 2, 0, 0, 0, 0, 0, 0, 0, // payload
        ];

        let error_message =
            receive_message(Chain::TestNet, &mut Cursor::new(&send_compact_message))
                .await
                .unwrap_err()
                .to_string();

        assert!(
            error_message.contains(
                "Received message for the wrong chain. Expected TestNet, Received RegTest."
            ),
            "Unexpected error message: {error_message}"
        );
    }

    #[tokio::test]
    async fn test_receive_message_invalid_payload_size() {
        let send_compact_message: Vec<u8> = vec![
            0xf9, 0xbe, 0xb4, 0xd9, // main chain
            b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0, // command
            0x05, 0, 0, 0, // payload size
            0xe9, 0x2f, 0x5e, 0xf8, // checksum
            0, 2, 0, 0, 0, 0, 0, 0, 0, // payload
        ];

        let error_message = receive_message(Chain::Main, &mut Cursor::new(&send_compact_message))
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Invalid payload size: Expected Exact(9). Received 5"),
            "Unexpected error message: {error_message}"
        );
    }

    #[tokio::test]
    async fn test_receive_message_payload_too_big() {
        let send_compact_message: Vec<u8> = vec![
            0xf9, 0xbe, 0xb4, 0xd9, // main chain
            b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0, // command
            0x09, 0, 0, 0, // payload size
            0xe9, 0x2f, 0x5e, 0xf8, // checksum
            0, 2, 0, 0, 0, 0, 0, 0, 0, // payload
            // extra bytes to try to overflow
            0, 0, // repeat message
            0xf9, 0xbe, 0xb4, 0xd9, // main chain
            b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0, // command
            0x09, 0, 0, 0, // payload size
            0xe9, 0x2f, 0x5e, 0xf8, // checksum
        ];

        let mut input_buffer = Cursor::new(&send_compact_message);

        let received = receive_message(Chain::Main, &mut input_buffer)
            .await
            .unwrap();

        let Message::SendCompact(message) = received else {
            panic!("Deserialized to wrong type");
        };

        assert_eq!(message.announce, false);
        assert_eq!(message.version, 2);

        let error_message = receive_message(Chain::Main, &mut input_buffer)
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Invalid chain id: 0x0000F9BE"),
            "Unexpected error message: {error_message}"
        );
    }

    #[tokio::test]
    async fn test_receive_message_wrong_checksum() {
        let send_compact_message: Vec<u8> = vec![
            0xf9, 0xbe, 0xb4, 0xd9, // main chain
            b's', b'e', b'n', b'd', b'c', b'm', b'p', b'c', b't', 0, 0, 0, // command
            0x09, 0, 0, 0, // payload size
            0xe9, 0x2f, 0x5e, 0xf9, // checksum
            0, 2, 0, 0, 0, 0, 0, 0, 0, // payload
        ];

        let mut input_buffer = Cursor::new(&send_compact_message);

        let error_message = receive_message(Chain::Main, &mut input_buffer)
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Received 0xE92F5EF9, Expected 0xE92F5EF8"),
            "Unexpected error message: {error_message}"
        );
    }
}
