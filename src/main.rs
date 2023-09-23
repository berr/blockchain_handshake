use crate::connection::{BitcoinConnection, ClientConfig};
use crate::networking::Chain;
use anyhow::Result;
use std::net::IpAddr;

pub mod connection;
pub mod message;
pub mod networking;
pub mod serialization;

const TESTING_VERSION: i32 = 70016;
const SUPPORTED_SERVICES: u64 = 1;
const USER_AGENT: &str = "/silveira-eiger:1.0/";

async fn connect_to_node(us: (IpAddr, u16), them: (IpAddr, u16)) -> Result<()> {
    let config = ClientConfig {
        chain: Chain::Main,
        version: TESTING_VERSION,
        services: SUPPORTED_SERVICES,
        user_agent: USER_AGENT.to_string(),
    };

    println!("Starting handshake");
    let mut connection = BitcoinConnection::start_tcp(config, us, them).await?;
    println!("Handshake finished");

    connection
        .ping()
        .await
        .expect("Ping should finish correctly");

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::message::VersionMessage;
    use crate::networking::send_message;

    #[tokio::test]
    async fn test_serialization() {
        let target_ip = "10.0.0.1".parse().unwrap();
        let target_port = 8333;

        let our_ip = "10.0.0.2".parse().unwrap();
        let our_port = 8333;

        let config = ClientConfig {
            chain: Chain::Main,
            version: 60002,
            services: 1,
            user_agent: "/Satoshi:0.7.2/".to_string(),
        };

        let message = VersionMessage::new(
            &config,
            (our_ip, our_port),
            (target_ip, target_port),
            0,
            false,
        );

        let mut output_buffer = vec![];
        send_message(&mut output_buffer, config.chain, message)
            .await
            .unwrap();

        assert_eq!(output_buffer[0..4], [0xF9u8, 0xBE, 0xB4, 0xD9]);
        assert_eq!(
            output_buffer[4..16],
            [0x76, 0x65, 0x72, 0x73, 0x69, 0x6F, 0x6E, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(output_buffer[16..20], [0x65, 0x00, 0x00, 0x00]);
        // assert_eq!(output_buffer[20..24], [0x65, 0x00, 0x00, 0x00]); -- checksum

        assert_eq!(output_buffer[24..28], [0x62u8, 0xEA, 0x00, 0x00]);
        assert_eq!(
            output_buffer[28..36],
            [0x01u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            output_buffer[36..44],
            [0x0Du8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );

        assert_eq!(
            output_buffer[44..52],
            [0x00u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            output_buffer[52..68],
            [
                0x00u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x0A,
                0x00, 0x00, 0x01
            ]
        );
        assert_eq!(output_buffer[68..70], [0x20u8, 0x8D]);

        assert_eq!(
            output_buffer[70..78],
            [0x01u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            output_buffer[78..94],
            [
                0x00u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x0A,
                0x00, 0x00, 0x02
            ]
        );
        assert_eq!(output_buffer[94..96], [0x20u8, 0x8D]);

        assert_eq!(
            output_buffer[96..104],
            [0x11u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            output_buffer[104..120],
            [
                0x0F, 0x2F, 0x53, 0x61, 0x74, 0x6F, 0x73, 0x68, 0x69, 0x3A, 0x30, 0x2E, 0x37, 0x2E,
                0x32, 0x2F
            ]
        );

        assert_eq!(output_buffer[120..124], [0x00, 0x00, 0x00, 0x00]);
        assert_eq!(output_buffer[124..125], [0x00]);

        assert_eq!(output_buffer.len(), 125);
    }

    #[test]
    fn cstring_has_nul() {
        let s = "test";
        let cstring = CString::new(s).unwrap();
        assert_eq!(cstring.as_bytes().len(), 4);
        assert_eq!(cstring.as_bytes_with_nul().len(), 5);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 51.89.178.149:8333

    let our_ip = "10.102.3.0".parse().unwrap();
    // let our_ip = "189.4.107.190".parse().unwrap();
    let our_port = 8333;

    // let target_ip = "172.17.0.2".parse().unwrap();
    let target_ip = "179.95.6.186".parse().unwrap();
    let target_port = 8333;

    connect_to_node((our_ip, our_port), (target_ip, target_port)).await?;

    Ok(())
}
