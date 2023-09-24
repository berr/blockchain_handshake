use crate::connection::ClientConfig;
use crate::serialization::{CompactSizeUint, DeserializePayload, SerializePayload};
use anyhow::{bail, Result};
use std::io::{Cursor, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::SystemTime;

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

    pub fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        match self {
            Message::Version(inner) => inner.serialize(buffer),
            Message::VersionAck(inner) => inner.serialize(buffer),
            Message::Ping(inner) => inner.serialize(buffer),
            Message::Pong(inner) => inner.serialize(buffer),
            Message::SendCompact(inner) => inner.serialize(buffer),
            Message::GetHeaders(inner) => inner.serialize(buffer),
            Message::FeeFilter(inner) => inner.serialize(buffer),
            Message::WTxIdRelay(inner) => inner.serialize(buffer),
            Message::SendAddrV2(inner) => inner.serialize(buffer),
        }
    }

    pub fn deserialize(kind: MessageKind, payload: Vec<u8>) -> Result<Self> {
        let mut cursor = Cursor::new(payload.as_slice());
        let message: Message = match kind {
            MessageKind::Version => VersionMessage::deserialize(&mut cursor)?.into(),
            MessageKind::VersionAck => VersionAckMessage::deserialize(&mut cursor)?.into(),
            MessageKind::Ping => PingMessage::deserialize(&mut cursor)?.into(),
            MessageKind::Pong => PongMessage::deserialize(&mut cursor)?.into(),
            MessageKind::SendCompact => SendCompactMessage::deserialize(&mut cursor)?.into(),
            MessageKind::GetHeaders => GetHeadersMessage::deserialize(&mut cursor)?.into(),
            MessageKind::FeeFilter => FeeFilterMessage::deserialize(&mut cursor)?.into(),
            MessageKind::WTxIdRelay => WTxIdRelayMessage::deserialize(&mut cursor)?.into(),
            MessageKind::SendAddrV2 => SendAddrV2Message::deserialize(&mut cursor)?.into(),
        };

        let remaining_bytes = payload.len() - cursor.position() as usize;
        if remaining_bytes > 0 {
            bail!("Deserialization didn't consume whole payload. {remaining_bytes} bytes left.");
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VersionMessage {
    pub version: i32,
    pub services: u64,
    pub timestamp: SystemTime,
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
        us: SocketAddr,
        them: SocketAddr,
        start_height: i32,
        relay: bool,
    ) -> Self {
        VersionMessage {
            version: config.version,
            services: config.services,
            timestamp: SystemTime::now(),
            addr_recv: NetworkAddressForVersion::new(them, 0),
            addr_trans: NetworkAddressForVersion::new(us, config.services),
            nonce: rand::random(),
            user_agent: config.user_agent.clone(),
            start_height,
            relay,
        }
    }
}

impl SerializePayload for VersionMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        self.version.serialize(buffer)?;
        self.services.serialize(buffer)?;
        self.timestamp.serialize(buffer)?;
        self.addr_recv.serialize(buffer)?;
        self.addr_trans.serialize(buffer)?;
        self.nonce.serialize(buffer)?;
        self.user_agent.serialize(buffer)?;
        self.start_height.serialize(buffer)?;
        self.relay.serialize(buffer)?;

        Ok(())
    }
}

impl DeserializePayload for VersionMessage {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let version = i32::deserialize(buffer)?;
        let services = u64::deserialize(buffer)?;
        let timestamp = SystemTime::deserialize(buffer)?;
        let addr_recv = NetworkAddressForVersion::deserialize(buffer)?;
        let addr_trans = NetworkAddressForVersion::deserialize(buffer)?;
        let nonce = u64::deserialize(buffer)?;
        let user_agent = String::deserialize(buffer)?;
        let start_height = i32::deserialize(buffer)?;
        let relay = bool::deserialize(buffer)?;

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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct NetworkAddressForVersion {
    pub services: u64,
    pub ip: IpAddr,
    pub port: Port,
}

impl NetworkAddressForVersion {
    pub fn new(address: SocketAddr, services: u64) -> Self {
        Self {
            services,
            ip: address.ip(),
            port: Port(address.port()),
        }
    }

    pub fn null() -> Self {
        Self {
            services: 0,
            ip: Ipv4Addr::new(0, 0, 0, 0).into(),
            port: Port(0),
        }
    }
}

impl SerializePayload for NetworkAddressForVersion {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        self.services.serialize(buffer)?;
        self.ip.serialize(buffer)?;
        self.port.serialize(buffer)?;

        Ok(())
    }
}

impl DeserializePayload for NetworkAddressForVersion {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let services = u64::deserialize(buffer)?;
        let ip = IpAddr::deserialize(buffer)?;
        let port = Port::deserialize(buffer)?;

        Ok(Self { services, ip, port })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Port(pub u16);

#[derive(Copy, Clone, Debug)]
pub struct VersionAckMessage;

impl SerializePayload for VersionAckMessage {
    fn serialize(&self, _: &mut Vec<u8>) -> Result<()> {
        // Nothing to serialize
        Ok(())
    }
}

impl DeserializePayload for VersionAckMessage {
    fn deserialize(_: &mut Cursor<&[u8]>) -> Result<Self> {
        // Nothing to deserialize
        Ok(VersionAckMessage)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PingMessage(pub u64);

impl SerializePayload for PingMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        self.0.serialize(buffer)
    }
}

impl DeserializePayload for PingMessage {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(PingMessage(u64::deserialize(buffer)?))
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PongMessage(pub u64);

impl SerializePayload for PongMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        self.0.serialize(buffer)
    }
}
impl DeserializePayload for PongMessage {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(PongMessage(u64::deserialize(buffer)?))
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SendCompactMessage {
    pub announce: bool,
    pub version: u64,
}

impl SerializePayload for SendCompactMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        self.announce.serialize(buffer)?;
        self.version.serialize(buffer)?;

        Ok(())
    }
}

impl DeserializePayload for SendCompactMessage {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let announce = bool::deserialize(buffer)?;
        let version = u64::deserialize(buffer)?;
        Ok(Self { announce, version })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BlockHeaderHash {
    pub hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct GetHeadersMessage {
    #[allow(dead_code)]
    pub version: i32,
    #[allow(dead_code)]
    pub hashes: Vec<BlockHeaderHash>,
    #[allow(dead_code)]
    pub stop_hash: BlockHeaderHash,
}

impl SerializePayload for GetHeadersMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        self.version.serialize(buffer)?;

        // Implementing manually here because implementing generically for Vec<T> can lead to
        // problems due to possible optional attributes
        CompactSizeUint(self.hashes.len() as u64).serialize(buffer)?;
        for h in &self.hashes {
            h.serialize(buffer)?;
        }
        self.stop_hash.serialize(buffer)?;

        Ok(())
    }
}

impl DeserializePayload for GetHeadersMessage {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let version = i32::deserialize(buffer)?;
        let number_of_hashes = CompactSizeUint::deserialize(buffer)?.0;
        let mut hashes = vec![];
        for _ in 0..number_of_hashes {
            let hash = BlockHeaderHash::deserialize(buffer)?;
            hashes.push(hash);
        }
        let stop_hash = BlockHeaderHash::deserialize(buffer)?;
        Ok(Self {
            version,
            hashes,
            stop_hash,
        })
    }
}

impl SerializePayload for BlockHeaderHash {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.write_all(&self.hash)?;
        Ok(())
    }
}

impl DeserializePayload for BlockHeaderHash {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let mut hash = [0u8; 32];
        buffer.read_exact(&mut hash)?;
        Ok(Self { hash })
    }
}

#[derive(Clone, Debug)]
pub struct FeeFilterMessage {
    pub fee_rate: u64,
}

impl SerializePayload for FeeFilterMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        self.fee_rate.serialize(buffer)
    }
}

impl DeserializePayload for FeeFilterMessage {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let fee_rate = u64::deserialize(buffer)?;
        Ok(Self { fee_rate })
    }
}

#[derive(Clone, Debug)]
pub struct WTxIdRelayMessage;

impl SerializePayload for WTxIdRelayMessage {
    fn serialize(&self, _: &mut Vec<u8>) -> Result<()> {
        // Nothing to serialize
        Ok(())
    }
}

impl DeserializePayload for WTxIdRelayMessage {
    fn deserialize(_: &mut Cursor<&[u8]>) -> Result<Self> {
        // Nothing to deserialize
        Ok(Self)
    }
}

#[derive(Clone, Debug)]
pub struct SendAddrV2Message;

impl SerializePayload for SendAddrV2Message {
    fn serialize(&self, _: &mut Vec<u8>) -> Result<()> {
        // Nothing to serialize
        Ok(())
    }
}

impl DeserializePayload for SendAddrV2Message {
    fn deserialize(_: &mut Cursor<&[u8]>) -> Result<Self> {
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
