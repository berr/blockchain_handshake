use crate::connection::ClientConfig;
use crate::serialization::{CompactSizeUint, DeserializePayload, SerializePayload};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::io::Cursor;
use std::iter;
use std::net::{IpAddr, Ipv6Addr};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Clone, Debug)]
pub enum Message {
    Version(VersionMessage),
    VersionAck(VersionAckMessage),
    Ping(PingMessage),
    Pong(PongMessage),
    SendCompact(SendCompactMessage),
    GetHeaders(GetHeadersMessage),
    FeeFilter(FeeFilterMessage),
    WTxIdRelay(WTxIdRelayMessage),
    SendAddrV2(SendAddrV2Message),
}

impl Message {
    pub fn kind(&self) -> MessageKind {
        match self {
            Message::Version(_) => MessageKind::Version,
            Message::VersionAck(_) => MessageKind::VersionAck,
            Message::Ping(_) => MessageKind::Ping,
            Message::Pong(_) => MessageKind::Pong,
            Message::SendCompact(_) => MessageKind::SendCompact,
            Message::GetHeaders(_) => MessageKind::GetHeaders,
            Message::FeeFilter(_) => MessageKind::FeeFilter,
            Message::WTxIdRelay(_) => MessageKind::WTxIdRelay,
            Message::SendAddrV2(_) => MessageKind::SendAddrV2,
        }
    }

    pub async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        match self {
            Message::Version(inner) => inner.serialize(writer).await,
            Message::VersionAck(inner) => inner.serialize(writer).await,
            Message::Ping(inner) => inner.serialize(writer).await,
            Message::Pong(inner) => inner.serialize(writer).await,
            Message::SendCompact(inner) => inner.serialize(writer).await,
            Message::GetHeaders(inner) => inner.serialize(writer).await,
            Message::FeeFilter(inner) => inner.serialize(writer).await,
            Message::WTxIdRelay(inner) => inner.serialize(writer).await,
            Message::SendAddrV2(inner) => inner.serialize(writer).await,
        }
    }

    pub async fn deserialize(kind: MessageKind, payload: Vec<u8>) -> Result<Self> {
        let mut cursor = Cursor::new(&payload);
        let message: Message = match kind {
            MessageKind::Version => VersionMessage::deserialize(&mut cursor).await?.into(),
            MessageKind::VersionAck => VersionAckMessage::deserialize(&mut cursor).await?.into(),
            MessageKind::Ping => PingMessage::deserialize(&mut cursor).await?.into(),
            MessageKind::Pong => PongMessage::deserialize(&mut cursor).await?.into(),
            MessageKind::SendCompact => SendCompactMessage::deserialize(&mut cursor).await?.into(),
            MessageKind::GetHeaders => GetHeadersMessage::deserialize(&mut cursor).await?.into(),
            MessageKind::FeeFilter => FeeFilterMessage::deserialize(&mut cursor).await?.into(),
            MessageKind::WTxIdRelay => WTxIdRelayMessage::deserialize(&mut cursor).await?.into(),
            MessageKind::SendAddrV2 => SendAddrV2Message::deserialize(&mut cursor).await?.into(),
        };

        let remaining_bytes = payload.len() - cursor.position() as usize;
        if remaining_bytes > 0 {
            println!("Deserialization didn't consume whole payload. {remaining_bytes} bytes left.");
        }

        Ok(message)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MessageKind {
    Version,
    VersionAck,
    Ping,
    Pong,
    SendCompact,
    GetHeaders,
    FeeFilter,
    WTxIdRelay,
    SendAddrV2,
}

#[derive(Debug, Clone)]
pub struct VersionMessage {
    pub version: i32,
    pub services: u64,
    pub timestamp: i64,
    pub addr_recv: NetworkAddressForVersion,
    pub addr_trans: NetworkAddressForVersion,
    pub nonce: u64,
    pub user_agent: String,
    pub start_height: i32,
    pub relay: bool,
}

impl VersionMessage {
    pub fn new(
        config: &ClientConfig,
        us: (IpAddr, u16),
        them: (IpAddr, u16),
        start_height: i32,
        relay: bool,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Current timestamp should be after unix epoch");
        let nonce = rand::random();

        VersionMessage {
            version: config.version,
            services: config.services,
            timestamp: timestamp.as_secs() as i64,
            addr_recv: NetworkAddressForVersion::from_address_and_services(them, 0),
            addr_trans: NetworkAddressForVersion::from_address_and_services(us, config.services),
            nonce,
            user_agent: config.user_agent.clone(),
            start_height,
            relay,
        }
    }
}

#[async_trait]
impl SerializePayload for VersionMessage {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        writer.write_i32(self.version.to_be()).await?;
        writer.write_u64(self.services.to_be()).await?;
        writer.write_i64(self.timestamp.to_be()).await?;
        self.addr_recv.serialize(writer).await?;
        self.addr_trans.serialize(writer).await?;
        writer.write_u64(self.nonce.to_be()).await?;
        writer
            .write_all(&[self.user_agent.as_bytes().len() as u8])
            .await?;
        writer.write_all(self.user_agent.as_bytes()).await?;
        writer.write_i32(self.start_height.to_be()).await?;
        writer.write_all(&[self.relay as u8]).await?;

        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for VersionMessage {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        let version = reader.read_i32_le().await?;
        let services = reader.read_u64_le().await?;
        let timestamp = reader.read_i64_le().await?;

        let addr_recv = NetworkAddressForVersion::deserialize(reader).await?;
        let addr_trans = NetworkAddressForVersion::deserialize(reader).await?;

        let nonce = reader.read_u64().await?;

        let user_agent_size = CompactSizeUint::deserialize(reader).await?.0;
        let mut user_agent_buffer: Vec<u8> =
            iter::repeat(0u8).take(user_agent_size as usize).collect();
        reader.read_exact(&mut user_agent_buffer).await?;
        let user_agent = String::from_utf8(user_agent_buffer)?;

        let start_height = reader.read_i32().await?;

        let relay = match reader.read_u8().await? {
            0 => false,
            1 => true,
            _ => bail!("boolean value must be either 0 or 1"),
        };

        Ok(Self {
            version,
            services,
            timestamp,
            addr_recv,
            addr_trans,
            nonce,
            user_agent,
            start_height,
            relay,
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct NetworkAddressForVersion {
    pub services: u64,
    pub ip: [u8; 16],
    pub port: u16,
}

impl NetworkAddressForVersion {
    pub fn from_address_and_services(address: (IpAddr, u16), services: u64) -> Self {
        let (address, port) = address;

        let as_ipv6 = match address {
            IpAddr::V4(ip) => {
                let mut converted = ip.to_ipv6_compatible().octets();
                converted[11] = 0xFF;
                converted[10] = 0xFF;
                Ipv6Addr::from(converted)
            }
            IpAddr::V6(ip) => ip,
        };

        Self {
            services,
            ip: as_ipv6.octets(),
            port,
        }
    }
}

#[async_trait]
impl SerializePayload for NetworkAddressForVersion {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        writer.write_u64_le(self.services).await?;
        writer.write_all(self.ip.as_slice()).await?;
        writer.write_u16(self.port).await?;

        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for NetworkAddressForVersion {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        let services = reader.read_u64_le().await?;
        let mut ip_buffer = [0u8; 16];
        reader.read_exact(&mut ip_buffer).await?;
        let port = reader.read_u16().await?;

        Ok(Self {
            services,
            ip: ip_buffer,
            port,
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct VersionAckMessage;

#[async_trait]
impl SerializePayload for VersionAckMessage {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, _: &mut W) -> Result<()> {
        // Nothing to serialize
        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for VersionAckMessage {
    async fn deserialize<R: AsyncRead + Unpin + Send>(_: &mut R) -> Result<Self> {
        // Nothing to deserialize
        Ok(VersionAckMessage)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PingMessage(pub u64);

#[async_trait]
impl SerializePayload for PingMessage {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        writer.write_u64(self.0).await?;
        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for PingMessage {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        Ok(PingMessage(reader.read_u64().await?))
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PongMessage(pub u64);

#[async_trait]
impl DeserializePayload for PongMessage {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        Ok(PongMessage(reader.read_u64().await?))
    }
}

#[async_trait]
impl SerializePayload for PongMessage {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        writer.write_u64(self.0).await?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SendCompactMessage {
    pub announce: bool,
    pub version: u64,
}

#[async_trait]
impl SerializePayload for SendCompactMessage {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        writer.write_u8(self.announce as u8).await?;
        writer.write_u64_le(self.version).await?;

        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for SendCompactMessage {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        let announce = reader.read_u8().await? == 0x1;
        let version = reader.read_u64_le().await?;
        Ok(Self { announce, version })
    }
}

#[derive(Copy, Clone, Debug)]
struct BlockHeaderHash {
    #[allow(dead_code)]
    hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct GetHeadersMessage {
    version: u32,
    hashes: Vec<BlockHeaderHash>,
    stop_hash: BlockHeaderHash,
}

#[async_trait]
impl SerializePayload for GetHeadersMessage {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        todo!()
    }
}

#[async_trait]
impl DeserializePayload for GetHeadersMessage {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        let version = reader.read_u32_le().await?;
        let number_of_hashes = CompactSizeUint::deserialize(reader).await?.0;
        let mut hashes = vec![];
        for _ in 0..number_of_hashes {
            let hash = BlockHeaderHash::deserialize(reader).await?;
            hashes.push(hash);
        }
        let stop_hash = BlockHeaderHash::deserialize(reader).await?;
        Ok(Self {
            version,
            hashes,
            stop_hash,
        })
    }
}

#[async_trait]
impl SerializePayload for BlockHeaderHash {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        todo!()
    }
}

#[async_trait]
impl DeserializePayload for BlockHeaderHash {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        let mut hash = [0u8; 32];
        reader.read_exact(&mut hash).await?;
        Ok(Self { hash })
    }
}

#[derive(Clone, Debug)]
pub struct FeeFilterMessage {
    fee_rate: u64,
}

#[async_trait]
impl SerializePayload for FeeFilterMessage {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        writer.write_u64_le(self.fee_rate).await?;
        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for FeeFilterMessage {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        let fee_rate = reader.read_u64_le().await?;
        Ok(Self { fee_rate })
    }
}

#[derive(Clone, Debug)]
pub struct WTxIdRelayMessage;

#[async_trait]
impl SerializePayload for WTxIdRelayMessage {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, _: &mut W) -> Result<()> {
        // Nothing to serialize
        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for WTxIdRelayMessage {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        // Nothing to deserialize
        Ok(Self)
    }
}

#[derive(Clone, Debug)]
pub struct SendAddrV2Message;

#[async_trait]
impl SerializePayload for SendAddrV2Message {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, _: &mut W) -> Result<()> {
        // Nothing to serialize
        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for SendAddrV2Message {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        // Nothing to deserialize
        Ok(Self)
    }
}

impl From<VersionMessage> for Message {
    fn from(value: VersionMessage) -> Self {
        Self::Version(value)
    }
}

impl From<VersionAckMessage> for Message {
    fn from(value: VersionAckMessage) -> Self {
        Self::VersionAck(value)
    }
}

impl From<PingMessage> for Message {
    fn from(value: PingMessage) -> Self {
        Self::Ping(value)
    }
}

impl From<PongMessage> for Message {
    fn from(value: PongMessage) -> Self {
        Self::Pong(value)
    }
}

impl From<SendCompactMessage> for Message {
    fn from(value: SendCompactMessage) -> Self {
        Self::SendCompact(value)
    }
}

impl From<GetHeadersMessage> for Message {
    fn from(value: GetHeadersMessage) -> Self {
        Self::GetHeaders(value)
    }
}

impl From<FeeFilterMessage> for Message {
    fn from(value: FeeFilterMessage) -> Self {
        Self::FeeFilter(value)
    }
}

impl From<WTxIdRelayMessage> for Message {
    fn from(value: WTxIdRelayMessage) -> Self {
        Self::WTxIdRelay(value)
    }
}

impl From<SendAddrV2Message> for Message {
    fn from(value: SendAddrV2Message) -> Self {
        Self::SendAddrV2(value)
    }
}
