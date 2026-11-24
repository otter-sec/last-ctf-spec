use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use asteroids_protocol::{ClientMessage, Controls, PROTOCOL_VERSION, Reassembler, ServerMessage, decode, encode};
use bevy::prelude::Resource;
use crossbeam_channel::{Receiver, Sender};
use quinn::crypto::rustls::QuicClientConfig;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

const ALPN: &[u8] = b"asteroids";
const QUIC_PORT: u16 = 443;
const UDP_PORT: u16 = 8443;
const HELLO_INTERVAL: Duration = Duration::from_millis(250);
const SERVER_TIMEOUT: Duration = Duration::from_secs(10);

type Error = Box<dyn std::error::Error>;

pub enum NetEvent {
    Message(ServerMessage),
    Rtt(u32),
    Closed(String),
}

#[derive(Resource)]
pub struct Connection {
    events: Receiver<NetEvent>,
    controls: mpsc::UnboundedSender<Controls>,
}

impl Connection {
    pub fn open(server: String, key: String) -> Self {
        let (events_tx, events) = crossbeam_channel::unbounded();
        let (controls, controls_rx) = mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("net".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("building the network runtime");
                let reason = match runtime.block_on(session(&server, &key, &events_tx, controls_rx)) {
                    Ok(()) => "connection closed".to_string(),
                    Err(err) => err.to_string(),
                };
                let _ = events_tx.send(NetEvent::Closed(reason));
            })
            .expect("spawning the network thread");
        Self { events, controls }
    }

    pub fn poll(&self) -> impl Iterator<Item = NetEvent> + '_ {
        self.events.try_iter()
    }

    pub fn send(&self, controls: Controls) {
        let _ = self.controls.send(controls);
    }
}

enum Link {
    Quic(quinn::Connection),
    Udp(UdpSocket),
}

impl Link {
    async fn open(server: &str) -> Result<Self, Error> {
        let (plain, server) = match server.strip_prefix("udp://") {
            Some(rest) => (true, rest),
            None => (false, server),
        };
        let (host, port) = match server.rsplit_once(':') {
            Some((host, port)) => (host, port.parse()?),
            None => (server, if plain { UDP_PORT } else { QUIC_PORT }),
        };
        let addr = tokio::net::lookup_host((host, port))
            .await?
            .find(SocketAddr::is_ipv4)
            .ok_or_else(|| format!("{host} has no address"))?;
        let bind = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0));

        if plain {
            let socket = UdpSocket::bind(bind).await?;
            socket.connect(addr).await?;
            return Ok(Self::Udp(socket));
        }
        let mut endpoint = quinn::Endpoint::client(bind)?;
        endpoint.set_default_client_config(quic_config());
        Ok(Self::Quic(endpoint.connect(addr, host)?.await?))
    }

    async fn send(&self, packet: Vec<u8>) -> Result<(), Error> {
        match self {
            Self::Quic(connection) => connection.send_datagram(packet.into())?,
            Self::Udp(socket) => {
                socket.send(&packet).await?;
            }
        }
        Ok(())
    }

    async fn receive(&self) -> Result<Vec<u8>, Error> {
        match self {
            Self::Quic(connection) => Ok(connection.read_datagram().await?.into()),
            Self::Udp(socket) => {
                let mut packet = vec![0; 2048];
                let len = socket.recv(&mut packet).await?;
                packet.truncate(len);
                Ok(packet)
            }
        }
    }

    fn rtt(&self) -> Option<Duration> {
        match self {
            Self::Quic(connection) => Some(connection.rtt()),
            Self::Udp(_) => None,
        }
    }
}

fn quic_config() -> quinn::ClientConfig {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("ring supports TLS 1.3")
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN.to_vec()];

    let mut transport = quinn::TransportConfig::default();
    transport
        .keep_alive_interval(Some(Duration::from_secs(5)))
        .max_idle_timeout(Some(
            Duration::from_secs(20).try_into().expect("20s is a valid idle timeout"),
        ));
    let mut config = quinn::ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(tls).expect("the ring provider has the QUIC initial suite"),
    ));
    config.transport_config(Arc::new(transport));
    config
}

async fn session(
    server: &str,
    key: &str,
    events: &Sender<NetEvent>,
    mut controls: mpsc::UnboundedReceiver<Controls>,
) -> Result<(), Error> {
    let link = Link::open(server).await?;
    let hello = encode(&ClientMessage::Hello {
        version: PROTOCOL_VERSION,
        key: key.to_string(),
    });

    let mut reassembler = Reassembler::default();
    let mut welcomed = false;
    let mut heard = Instant::now();
    let mut timer = tokio::time::interval(HELLO_INTERVAL);
    loop {
        tokio::select! {
            packet = link.receive() => {
                let Some(message) = reassembler.push(&packet?).and_then(|frame| decode(&frame)) else {
                    continue;
                };
                heard = Instant::now();
                welcomed |= matches!(message, ServerMessage::Welcome(_));
                let kicked = matches!(message, ServerMessage::Kick { .. });
                let _ = events.send(NetEvent::Message(message));
                if kicked {
                    return Ok(());
                }
            }
            Some(controls) = controls.recv() => {
                if welcomed {
                    link.send(encode(&ClientMessage::Input(controls))).await?;
                }
            }
            _ = timer.tick() => {
                if heard.elapsed() > SERVER_TIMEOUT {
                    return Err("the server stopped answering".into());
                }
                if !welcomed {
                    link.send(hello.clone()).await?;
                }
                if let Some(rtt) = link.rtt() {
                    let _ = events.send(NetEvent::Rtt(rtt.as_millis() as u32));
                }
            }
        }
    }
}
