mod config;
mod game;
mod replay;
mod server;

use std::process::ExitCode;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tracing::{error, info};

use crate::config::Config;
use crate::server::Server;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_target(false)
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!(%err, "fatal");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let socket = Arc::new(UdpSocket::bind(&config.listen).await?);
    info!(
        addr = %socket.local_addr()?,
        match_id = config.match_id,
        players = config.players.len(),
        duration_s = config.match_duration.as_secs(),
        "match server listening"
    );

    let server = Arc::new(Server::new(config, socket.clone()));
    tokio::spawn(server.clone().run());

    let shutdown = shutdown();
    tokio::pin!(shutdown);
    let mut packet = [0u8; 2048];
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            received = socket.recv_from(&mut packet) => {
                let (len, from) = received?;
                server.receive(&packet[..len], from);
            }
        }
    }
    Ok(())
}

async fn shutdown() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("installing the SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}
