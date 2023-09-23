use crate::connection::{BitcoinConnection, ClientConfig};
use anyhow::Result;
use std::net::IpAddr;

pub mod connection;
pub mod message;
pub mod networking;
pub mod serialization;
pub mod testing;

const USER_AGENT: &str = "/silveira-eiger:1.0/";

async fn connect_to_node(us: (IpAddr, u16), them: (IpAddr, u16)) -> Result<()> {
    let config = ClientConfig::default_with_user_agent(USER_AGENT.to_string());

    println!("Starting handshake");
    let mut connection = BitcoinConnection::start_tcp(config, us, them).await?;
    println!("Handshake finished");

    connection
        .ping()
        .await
        .expect("Ping should finish correctly");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // 51.89.178.149:8333

    let our_ip = "10.102.3.0".parse().unwrap();
    // let our_ip = "189.4.107.190".parse().unwrap();
    let our_port = 8333;

    let target_ip = "172.17.0.2".parse().unwrap();
    // let target_ip = "179.95.6.186".parse().unwrap();
    let target_port = 8333;

    connect_to_node((our_ip, our_port), (target_ip, target_port)).await?;

    Ok(())
}
