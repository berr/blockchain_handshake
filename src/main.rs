use crate::connection::{BlockchainConnection, ClientConfig};
use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;
use tokio::time::timeout;

pub mod connection;
pub mod message;
pub mod networking;
pub mod serialization;
pub mod testing;

const USER_AGENT: &str = "/silveira-blockchain-handshake:1.0/";

async fn connect_to_node(us: SocketAddr, them: SocketAddr) -> Result<()> {
    let config = ClientConfig::default_with_user_agent(USER_AGENT.to_string());

    println!("Starting handshake");
    let mut connection = BlockchainConnection::start_tcp(config, us, them).await?;
    println!("Handshake finished");

    println!("Sending a ping to make sure we're correctly connected");
    connection
        .ping()
        .await
        .expect("Ping should finish correctly");
    println!("Success!");
    println!("Closing up the application");

    Ok(())
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Parameters {
    target_ip: String,
    #[arg(long, default_value_t = 8333)]
    target_port: u16,

    #[arg(long, default_value=USER_AGENT)]
    user_agent: String,

    #[arg(long, default_value = "0.0.0.0")]
    local_ip: String,
    #[arg(long, default_value_t = 8333)]
    local_port: u16,

    #[arg(long, default_value_t = 60)]
    timeout_seconds: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let parameters = Parameters::parse();

    let local_address_str = format!("{}:{}", parameters.local_ip, parameters.local_port);
    let local_address = local_address_str
        .to_socket_addrs()
        .with_context(|| local_address_str)?
        .next()
        .ok_or_else(|| anyhow!("Couldn't resolve: {}", parameters.target_ip))?;

    let target_address_str = format!("{}:{}", parameters.target_ip, parameters.target_port);
    let target_address = target_address_str
        .to_socket_addrs()
        .with_context(|| target_address_str)?
        .next()
        .ok_or_else(|| anyhow!("Couldn't resolve: {}", parameters.target_ip))?;

    let run_handshake = connect_to_node(local_address, target_address);

    match timeout(
        Duration::from_secs(parameters.timeout_seconds),
        run_handshake,
    )
    .await
    {
        Ok(r) => r,
        Err(_) => bail!("Connection to remote host timed out"),
    }
}
