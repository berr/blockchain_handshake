use crate::message::{
    Message, MessageKind, PingMessage, PongMessage, SendAddrV2Message, VersionAckMessage,
    VersionMessage, WTxIdRelayMessage,
};
use crate::networking::{receive_message, send_message, Chain};
use anyhow::{bail, Result};
use std::net::IpAddr;
use tokio::io::{AsyncRead, AsyncWrite, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub chain: Chain,
    pub version: i32,
    pub services: u64,
    pub user_agent: String,
}

pub struct BitcoinConnection<R, W> {
    config: ClientConfig,
    #[allow(dead_code)]
    us: (IpAddr, u16),
    #[allow(dead_code)]
    them: (IpAddr, u16),
    reader: R,
    writer: W,
}

impl BitcoinConnection<BufReader<OwnedReadHalf>, BufWriter<OwnedWriteHalf>> {
    pub async fn start_tcp(
        config: ClientConfig,
        us: (IpAddr, u16),
        them: (IpAddr, u16),
    ) -> Result<Self> {
        let (reader, writer) = TcpStream::connect(them).await?.into_split();
        let reader = BufReader::new(reader);
        let writer = BufWriter::new(writer);

        BitcoinConnection::start(config, us, them, reader, writer).await
    }
}

impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> BitcoinConnection<R, W> {
    pub async fn start(
        config: ClientConfig,
        us: (IpAddr, u16),
        them: (IpAddr, u16),
        reader: R,
        writer: W,
    ) -> Result<Self> {
        Self::hand_shake(config, us, them, reader, writer).await
    }

    async fn hand_shake(
        config: ClientConfig,
        us: (IpAddr, u16),
        them: (IpAddr, u16),
        mut reader: R,
        mut writer: W,
    ) -> Result<Self> {
        let version_message = VersionMessage::new(&config, us, them, 0, false);
        let sent_nonce = version_message.nonce;
        send_message(&mut writer, config.chain, version_message).await?;

        let received = receive_message(config.chain, &mut reader).await?;

        let received_version = match received {
            Message::Version(v) => v,
            _ => bail!("Handshake failed. Expected to receive version but received: {received:?}"),
        };

        if received_version.version != config.version {
            bail!(
                "Handshake failed. Different version encountered. {0} != {1}",
                received_version.version,
                config.version
            )
        }

        if received_version.nonce == sent_nonce {
            bail!("Handshake failed. Sent nonce is the same as received. Possible self connection. {0} = {1}",
                received_version.nonce,
                sent_nonce,
            )
        }

        let received = receive_message(config.chain, &mut reader).await?;
        if received.kind() != MessageKind::WTxIdRelay {
            bail!("Handshake failed. Expected to receive WTxIdRelay but received: {received:?}");
        }

        let received = receive_message(config.chain, &mut reader).await?;
        if received.kind() != MessageKind::SendAddrV2 {
            bail!("Handshake failed. Expected to receive SendAddrV2 but received: {received:?}");
        }

        let received = receive_message(config.chain, &mut reader).await?;
        if received.kind() != MessageKind::VersionAck {
            bail!("Handshake failed. Expected to receive version ack but received: {received:?}");
        }

        send_message(&mut writer, config.chain, WTxIdRelayMessage).await?;
        send_message(&mut writer, config.chain, SendAddrV2Message).await?;
        send_message(&mut writer, config.chain, VersionAckMessage).await?;

        match receive_message(config.chain, &mut reader).await? {
            Message::SendCompact(_) => (), // it's ok to ignore the message just for handshaking
            m => bail!("Expected to receive 'sendcmpct' message, but received {m:?}"),
        };

        let received_ping = match receive_message(config.chain, &mut reader).await? {
            Message::Ping(msg) => msg,
            m => bail!("Expected to receive 'ping' message, but received {m:?}"),
        };

        send_message(&mut writer, config.chain, PongMessage(received_ping.0)).await?;

        let received = receive_message(config.chain, &mut reader).await?;
        if received.kind() != MessageKind::GetHeaders {
            bail!("Expected to receive 'getheaders' message, but received {received:?}")
        }

        let received = receive_message(config.chain, &mut reader).await?;
        if received.kind() != MessageKind::FeeFilter {
            bail!("Expected to receive 'getheaders' message, but received {received:?}")
        }

        let connection = BitcoinConnection {
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
