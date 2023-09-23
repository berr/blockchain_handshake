use crate::message::{Message, MessageKind};
use crate::testing::encode_bytes;
use anyhow::{bail, Result};
use std::iter;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum Chain {
    Main = 0xf9beb4d9,
    TestNet = 0x0b110907,
    RegTest = 0xfabfb5da,
}

/// Serialize message, add a header and send it through a Sender
///
/// Since we generated the message, there isn't anything to be defensive here.
pub async fn send_message<W: AsyncWrite + Unpin + Send, M: Into<Message>>(
    writer: &mut W,
    chain: Chain,
    message: M,
) -> Result<()> {
    let message = message.into();
    println!("Sending: {message:?}");
    let mut payload = vec![];
    message.serialize(&mut payload).await?;
    let checksum = bitcoin_checksum(&payload);

    writer.write_u32((chain as u32).to_le()).await?;
    writer.write_all(message.kind().command()).await?;
    writer.write_u32((payload.len() as u32).to_be()).await?;
    writer.write_u32(checksum.to_le()).await?;
    writer.write_all(&payload).await?;

    writer.flush().await?;

    Ok(())
}

/// Receive a message for the given chain
pub async fn receive_message<R: AsyncRead + Unpin + Send>(
    chain: Chain,
    reader: &mut R,
) -> Result<Message> {
    let mut header = [0u8; 24];
    reader.read_exact(&mut header).await?;
    let received_chain = u32::from_be_bytes(header[0..4].try_into().unwrap());

    if received_chain != chain as u32 {
        bail!("Received message for the wrong chain. ");
    }

    let mut command = [0u8; 12];
    command.copy_from_slice(&header[4..16]);
    let message_kind: MessageKind = command.try_into()?;

    let payload_size = u32::from_le_bytes(header[16..20].try_into().unwrap());

    if !message_kind.maximum_allowed_size().accepts(payload_size) {
        bail!("Invalid payload size: {payload_size}");
    }

    let received_checksum = u32::from_be_bytes(header[20..24].try_into().unwrap());

    let mut payload: Vec<u8> = iter::repeat(0).take(payload_size as usize).collect();
    reader.read_exact(payload.as_mut()).await?;

    println!("Header: {}", encode_bytes(&header));
    println!("Payload: {}", encode_bytes(&payload));

    let calculated_checksum = bitcoin_checksum(&payload);

    if received_checksum != calculated_checksum {
        bail!("Received invalid checksum");
    }

    let deserialized = Message::deserialize(message_kind, payload).await?;
    println!("Received: {deserialized:?}");

    Ok(deserialized)
}

fn bitcoin_checksum(data: &[u8]) -> u32 {
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
            // MessageKind::Headers => &commands::HEADERS,
        }
    }
}

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
