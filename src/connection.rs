use crate::message::{
    Message, MessageKind, PingMessage, PongMessage, SendAddrV2Message, VersionAckMessage,
    VersionMessage, WTxIdRelayMessage,
};
use crate::networking::{receive_message, send_message, Chain};
use anyhow::{bail, Result};
use log::trace;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
const SUPPORTED_VERSION: i32 = 70016;

// This should be a Bitfield type, but since we're always sending the same value and we're not checking for specific features, it's fine to just use a constant
const SUPPORTED_SERVICES: u64 = 1;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub chain: Chain,
    pub version: i32,
    pub services: u64,
    pub user_agent: String,
}

impl ClientConfig {
    pub fn default_with_user_agent(user_agent: String) -> Self {
        Self {
            chain: Chain::Main,
            version: SUPPORTED_VERSION,
            services: SUPPORTED_SERVICES,
            user_agent,
        }
    }
}

pub struct BlockchainConnection<R, W> {
    config: ClientConfig,
    #[allow(dead_code)]
    us: SocketAddr,
    #[allow(dead_code)]
    them: SocketAddr,
    reader: R,
    writer: W,
}

impl BlockchainConnection<BufReader<OwnedReadHalf>, BufWriter<OwnedWriteHalf>> {
    pub async fn start_tcp(config: ClientConfig, us: SocketAddr, them: SocketAddr) -> Result<Self> {
        let (reader, writer) = TcpStream::connect(them).await?.into_split();
        let reader = BufReader::new(reader);
        let writer = BufWriter::new(writer);

        BlockchainConnection::start(config, us, them, reader, writer).await
    }
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> BlockchainConnection<R, W> {
    pub async fn start(
        config: ClientConfig,
        us: SocketAddr,
        them: SocketAddr,
        reader: R,
        writer: W,
    ) -> Result<Self> {
        Self::handshake(config, us, them, reader, writer).await
    }

    async fn handshake(
        config: ClientConfig,
        us: SocketAddr,
        them: SocketAddr,
        mut reader: R,
        mut writer: W,
    ) -> Result<Self> {
        let mut state_machine = HandShakeStateMachine::start(config.clone(), us, them);

        while !state_machine.finished() {
            state_machine.next_state(&mut reader, &mut writer).await?;
        }

        let connection = BlockchainConnection {
            config,
            us,
            them,
            reader,
            writer,
        };

        Ok(connection)
    }

    pub async fn ping(&mut self) -> Result<()> {
        let ping_message = PingMessage(10);
        let ping_nonce = ping_message.0;
        send_message(&mut self.writer, self.config.chain, ping_message).await?;

        let received = receive_message(self.config.chain, &mut self.reader).await?;

        let pong_nonce = match received {
            Message::Pong(pong) => pong.0,
            _ => bail!("Expected Pong, but received {received:?}"),
        };

        if ping_nonce != pong_nonce {
            bail!("Ping and Pong nonces don't match. {ping_nonce} != {pong_nonce}");
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
struct HandShakeStateMachine {
    config: ClientConfig,
    state: Option<HandShakeState>,
}

impl HandShakeStateMachine {
    pub fn start(config: ClientConfig, us: SocketAddr, them: SocketAddr) -> Self {
        Self {
            config,
            state: Some(HandShakeState::ToSendVersion(us, them)),
        }
    }

    pub fn finished(&self) -> bool {
        self.state.is_none()
    }

    pub async fn next_state<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send>(
        &mut self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<()> {
        let Some(current_state) = self.state.as_ref() else {
            bail!("Handshake already finished");
        };

        let next_state = match current_state {
            HandShakeState::ToSendVersion(us, them) => {
                let message = VersionMessage::new(&self.config, *us, *them, 0, false);
                let sent_nonce = message.nonce;
                self.send_message(writer, message).await?;
                HandShakeState::ExpectingVersion(sent_nonce)
            }
            HandShakeState::ExpectingVersion(sent_nonce) => {
                let received = self.receive_message(reader).await?;
                let version = match received {
                    Message::Version(v) => v,
                    _ => bail!(
                        "Handshake failed. Expected to receive version but received: {received:?}"
                    ),
                };

                if version.version != self.config.version {
                    bail!(
                        "Handshake failed. Different version encountered. {0} != {1}",
                        version.version,
                        self.config.version
                    )
                }

                if version.nonce == *sent_nonce {
                    bail!(
                        "Handshake failed. Sent nonce is the same as received. Self connection. {0} = {1}",
                        version.nonce,
                        sent_nonce
                    )
                }
                HandShakeState::ExpectingWtxIdRelay
            }
            HandShakeState::ExpectingWtxIdRelay => {
                self.ignore_message(reader, MessageKind::WTxIdRelay).await?;
                HandShakeState::ExpectingSendAddrV2
            }
            HandShakeState::ExpectingSendAddrV2 => {
                self.ignore_message(reader, MessageKind::SendAddrV2).await?;
                HandShakeState::ExpectingVersionAck
            }
            HandShakeState::ExpectingVersionAck => {
                self.ignore_message(reader, MessageKind::VersionAck).await?;
                HandShakeState::ToSendWTxIdRelay
            }
            HandShakeState::ToSendWTxIdRelay => {
                self.send_message(writer, WTxIdRelayMessage).await?;
                HandShakeState::ToSendSendAddrV2
            }
            HandShakeState::ToSendSendAddrV2 => {
                self.send_message(writer, SendAddrV2Message).await?;
                HandShakeState::ToSendVersionAck
            }

            HandShakeState::ToSendVersionAck => {
                self.send_message(writer, VersionAckMessage).await?;
                HandShakeState::ExpectingSendCompact
            }
            HandShakeState::ExpectingSendCompact => {
                self.ignore_message(reader, MessageKind::SendCompact)
                    .await?;
                HandShakeState::ExpectingPing
            }
            HandShakeState::ExpectingPing => {
                let received = self.receive_message(reader).await?;
                let ping = match received {
                    Message::Ping(v) => v,
                    _ => bail!(
                        "Handshake failed. Expected to receive ping but received: {received:?}"
                    ),
                };
                HandShakeState::ToSendPong(ping.0)
            }
            HandShakeState::ToSendPong(nonce) => {
                self.send_message(writer, PongMessage(*nonce)).await?;
                HandShakeState::ExpectingGetHeaders
            }
            HandShakeState::ExpectingGetHeaders => {
                self.ignore_message(reader, MessageKind::GetHeaders).await?;
                HandShakeState::ExpectingFeeFilter
            }
            HandShakeState::ExpectingFeeFilter => {
                self.ignore_message(reader, MessageKind::FeeFilter).await?;
                self.state = None;
                return Ok(());
            }
        };

        self.state = Some(next_state);

        Ok(())
    }

    async fn receive_message<R: AsyncRead + Unpin + Send>(
        &self,
        reader: &mut R,
    ) -> Result<Message> {
        let message = receive_message(self.config.chain, reader).await?;
        trace!("Received {message:?}");
        Ok(message)
    }

    async fn send_message<W: AsyncWrite + Unpin + Send, M: Into<Message>>(
        &self,
        writer: &mut W,
        message: M,
    ) -> Result<()> {
        let message = message.into();
        trace!("Sending {message:?}");
        send_message(writer, self.config.chain, message).await
    }

    async fn ignore_message<R: AsyncRead + Unpin + Send>(
        &self,
        reader: &mut R,
        expected_kind: MessageKind,
    ) -> Result<()> {
        let received = self.receive_message(reader).await?;
        if received.kind() != expected_kind {
            bail!(
                "Handshake failed. Expected to receive {expected_kind:?} but received {received:?}"
            );
        }

        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
enum HandShakeState {
    ToSendVersion(SocketAddr, SocketAddr),
    ExpectingVersion(u64),
    ExpectingWtxIdRelay,
    ExpectingSendAddrV2,
    ExpectingVersionAck,
    ToSendWTxIdRelay,
    ToSendSendAddrV2,
    ToSendVersionAck,
    ExpectingSendCompact,
    ExpectingPing,
    ToSendPong(u64),
    ExpectingGetHeaders,
    ExpectingFeeFilter,
}

#[cfg(test)]
mod test {
    use crate::connection::{
        BlockchainConnection, ClientConfig, HandShakeStateMachine, SUPPORTED_SERVICES,
        SUPPORTED_VERSION,
    };

    use crate::message::{
        BlockHeaderHash, FeeFilterMessage, GetHeadersMessage, Message, MessageKind,
        NetworkAddressForVersion, PingMessage, PongMessage, SendAddrV2Message, SendCompactMessage,
        VersionAckMessage, VersionMessage, WTxIdRelayMessage,
    };
    use crate::networking::{receive_message, send_message, Chain};
    use crate::testing::decode_bytes;
    use anyhow::Result;
    use std::io::Cursor;
    use std::net::SocketAddr;
    use std::str::FromStr;

    fn testing_client_config() -> ClientConfig {
        ClientConfig::default_with_user_agent("TestingClient".to_string())
    }

    #[tokio::test]
    /// Replicates the network communication from a test using a local blockchain node
    /// Useful for ensuring binary stability
    async fn smoke_test_handshake() {
        let config = testing_client_config();

        let mut reader = vec![];
        let mut writer = vec![];

        let us = SocketAddr::from_str("10.102.3.0:8333").unwrap();
        let them = SocketAddr::from_str("172.17.0.2:8333").unwrap();

        reader.extend(decode_bytes("+b602XZlcnNpb24AAAAAAGYAAACx39uS"));
        reader.extend(decode_bytes("gBEBAAkEAAAAAAAAJGkPZQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAkEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAogitpThFWBwQL1NhdG9zaGk6MjUuMC4wLwAAAAAB"));

        reader.extend(decode_bytes("+b602Xd0eGlkcmVsYXkAAAAAAABd9uDi"));
        reader.extend(decode_bytes(""));

        reader.extend(decode_bytes("+b602XNlbmRhZGRydjIAAAAAAABd9uDi"));
        reader.extend(decode_bytes(""));

        reader.extend(decode_bytes("+b602XZlcmFjawAAAAAAAAAAAABd9uDi"));
        reader.extend(decode_bytes(""));

        reader.extend(decode_bytes("+b602XNlbmRjbXBjdAAAAAkAAADpL174"));
        reader.extend(decode_bytes("AAIAAAAAAAAA"));

        reader.extend(decode_bytes("+b602XBpbmcAAAAAAAAAAAgAAACwY0uh"));
        reader.extend(decode_bytes("IrahJ/vs17E"));

        reader.extend(decode_bytes("+b602WdldGhlYWRlcnMAAEUAAAAdNv5T"));
        reader.extend(decode_bytes("gBEBAAFv4owKtvGzcsGmokauY/dPkx6DZeFaCJxo1hkAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));

        reader.extend(decode_bytes("+b602WZlZWZpbHRlcgAAAAgAAABGhxVs"));
        reader.extend(decode_bytes("NfCLAAAAAAA"));

        let mut read_cursor = Cursor::new(&mut reader);
        let write_cursor = Cursor::new(&mut writer);

        BlockchainConnection::start(config, us, them, &mut read_cursor, write_cursor)
            .await
            .unwrap();

        // Should consume the whole input
        assert_eq!(read_cursor.position() as usize, reader.len());
    }

    #[tokio::test]
    async fn test_full_handshake_state_machine() {
        let config = testing_client_config();
        let us = SocketAddr::from_str("10.102.3.0:8333").unwrap();
        let them = SocketAddr::from_str("172.17.0.2:8333").unwrap();

        let mut sm = HandShakeStateMachine::start(config.clone(), us, them);

        let Message::Version(deserialized_version) =
            receive_from_sm(&mut sm, config.chain).await.unwrap()
        else {
            panic!("Expected version");
        };
        let expected_version = VersionMessage {
            version: SUPPORTED_VERSION,
            services: SUPPORTED_SERVICES,
            timestamp: deserialized_version.timestamp,
            addr_recv: NetworkAddressForVersion::new(them, 0),
            addr_trans: NetworkAddressForVersion::new(us, SUPPORTED_SERVICES),
            nonce: deserialized_version.nonce,
            user_agent: config.user_agent.clone(),
            start_height: 0,
            relay: false,
        };

        assert_eq!(deserialized_version, expected_version);

        let other_side_version = VersionMessage {
            version: SUPPORTED_VERSION,
            services: 1003,
            timestamp: deserialized_version.timestamp,
            addr_recv: NetworkAddressForVersion::null(),
            addr_trans: NetworkAddressForVersion::new(them, 1003),
            nonce: 123456789,
            user_agent: "Mocked".to_string(),
            start_height: 1,
            relay: true,
        };

        send_to_sm(&mut sm, config.chain, other_side_version)
            .await
            .unwrap();

        send_to_sm(&mut sm, config.chain, WTxIdRelayMessage)
            .await
            .unwrap();

        send_to_sm(&mut sm, config.chain, SendAddrV2Message)
            .await
            .unwrap();

        send_to_sm(&mut sm, config.chain, VersionAckMessage)
            .await
            .unwrap();

        assert_eq!(
            receive_from_sm(&mut sm, config.chain).await.unwrap().kind(),
            MessageKind::WTxIdRelay
        );
        assert_eq!(
            receive_from_sm(&mut sm, config.chain).await.unwrap().kind(),
            MessageKind::SendAddrV2
        );
        assert_eq!(
            receive_from_sm(&mut sm, config.chain).await.unwrap().kind(),
            MessageKind::VersionAck
        );

        let send_compact_message = SendCompactMessage {
            announce: false,
            version: 2,
        };
        send_to_sm(&mut sm, config.chain, send_compact_message)
            .await
            .unwrap();

        let ping_nonce = 987654321;
        send_to_sm(&mut sm, config.chain, PingMessage(ping_nonce))
            .await
            .unwrap();

        let Message::Pong(PongMessage(pong_nonce)) =
            receive_from_sm(&mut sm, config.chain).await.unwrap()
        else {
            panic!("Expected pong");
        };

        assert_eq!(ping_nonce, pong_nonce);

        let get_headers_message = GetHeadersMessage {
            version: SUPPORTED_VERSION,
            hashes: vec![BlockHeaderHash {
                hash: rand::random(),
            }],
            stop_hash: BlockHeaderHash {
                hash: rand::random(),
            },
        };
        send_to_sm(&mut sm, config.chain, get_headers_message)
            .await
            .unwrap();

        send_to_sm(&mut sm, config.chain, FeeFilterMessage { fee_rate: 123 })
            .await
            .unwrap();

        assert!(sm.finished());
    }

    #[tokio::test]
    async fn test_handkshake_fails_with_different_versions() {
        let config = testing_client_config();
        let us = SocketAddr::from_str("10.102.3.0:8333").unwrap();
        let them = SocketAddr::from_str("172.17.0.2:8333").unwrap();

        let mut sm = HandShakeStateMachine::start(config.clone(), us, them);

        let Message::Version(deserialized_version) =
            receive_from_sm(&mut sm, config.chain).await.unwrap()
        else {
            panic!("Expected version");
        };

        let other_side_version = VersionMessage {
            version: SUPPORTED_VERSION,
            services: 1003,
            timestamp: deserialized_version.timestamp,
            addr_recv: NetworkAddressForVersion::null(),
            addr_trans: NetworkAddressForVersion::new(them, 1003),
            nonce: 123456789,
            user_agent: "Mocked".to_string(),
            start_height: 1,
            relay: true,
        };

        let other_side_version_below = VersionMessage {
            version: SUPPORTED_VERSION - 1,
            ..other_side_version.clone()
        };

        let mut below_sm = sm.clone();
        let error_message = send_to_sm(&mut below_sm, config.chain, other_side_version_below)
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Different version encountered. 70015 != 70016"),
            "Unexpected error message: {}",
            error_message
        );

        let other_side_version_above = VersionMessage {
            version: SUPPORTED_VERSION + 1,
            ..other_side_version.clone()
        };
        let mut above_sm = sm.clone();
        let error_message = send_to_sm(&mut above_sm, config.chain, other_side_version_above)
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Different version encountered. 70017 != 70016"),
            "Unexpected error message: {}",
            error_message
        );
    }

    #[tokio::test]
    async fn test_handkshake_fails_with_repeated_nonce() {
        let config = testing_client_config();
        let us = SocketAddr::from_str("10.102.3.0:8333").unwrap();
        let them = SocketAddr::from_str("172.17.0.2:8333").unwrap();

        let mut sm = HandShakeStateMachine::start(config.clone(), us, them);

        let Message::Version(deserialized_version) =
            receive_from_sm(&mut sm, config.chain).await.unwrap()
        else {
            panic!("Expected version");
        };

        let other_side_version = VersionMessage {
            version: SUPPORTED_VERSION,
            services: 1003,
            timestamp: deserialized_version.timestamp,
            addr_recv: NetworkAddressForVersion::null(),
            addr_trans: NetworkAddressForVersion::new(them, 1003),
            nonce: deserialized_version.nonce,
            user_agent: "Mocked".to_string(),
            start_height: 1,
            relay: true,
        };

        let error_message = send_to_sm(&mut sm, config.chain, other_side_version)
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Sent nonce is the same as received."),
            "Unexpected error message: {}",
            error_message
        );
    }

    #[tokio::test]
    async fn test_handkshake_fails_when_receiving_unexpected_message() {
        let config = testing_client_config();
        let us = SocketAddr::from_str("10.102.3.0:8333").unwrap();
        let them = SocketAddr::from_str("172.17.0.2:8333").unwrap();

        let mut sm = HandShakeStateMachine::start(config.clone(), us, them);
        receive_from_sm(&mut sm, config.chain).await.unwrap();

        let error_message = send_to_sm(&mut sm, config.chain, PingMessage(32))
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Expected to receive version but received: Ping"),
            "Unexpected error message: {}",
            error_message,
        );
    }

    async fn send_to_sm<M: Into<Message>>(
        sm: &mut HandShakeStateMachine,
        chain: Chain,
        msg: M,
    ) -> Result<()> {
        let mut input = vec![];
        send_message(&mut input, chain, msg).await?;
        let mut output = vec![];

        sm.next_state(&mut Cursor::new(&input), &mut output).await?;

        assert!(output.is_empty());
        Ok(())
    }

    async fn receive_from_sm(sm: &mut HandShakeStateMachine, chain: Chain) -> Result<Message> {
        let mut input = vec![];
        let mut output = vec![];

        sm.next_state(&mut Cursor::new(&mut input), &mut output)
            .await?;

        let deserialized = receive_message(chain, &mut Cursor::new(output)).await?;
        assert!(input.is_empty());
        Ok(deserialized)
    }
}
