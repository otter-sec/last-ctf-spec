use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use asteroids_protocol::{
    ClientMessage, Controls, PROTOCOL_VERSION, Phase, ReplayWriter, SNAPSHOT_INTERVAL_TICKS, ServerMessage, TICK_RATE,
    decode, encode, fragment,
};
use serde_json::json;
use tokio::net::UdpSocket;
use tokio::time::{Instant, MissedTickBehavior, interval};
use tracing::{info, warn};

use crate::config::Config;
use crate::game::Game;
use crate::replay;

const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_INPUTS_PER_SECOND: u32 = 10 * TICK_RATE;
const GAME_OVER_INTERVAL_TICKS: u32 = TICK_RATE / 2;

struct Finished {
    replay: Vec<u8>,
    scoreboard: String,
}

struct Connection {
    addr: SocketAddr,
    heard: Instant,
    window: Instant,
    inputs: u32,
}

struct State {
    game: Game,
    connections: Vec<Option<Connection>>,
    replay: Option<ReplayWriter>,
    socket: Arc<UdpSocket>,
    messages: u32,
}

pub struct Server {
    config: Config,
    state: Mutex<State>,
}

impl Server {
    pub fn new(config: Config, socket: Arc<UdpSocket>) -> Self {
        let game = Game::new(&config);
        let mut replay = ReplayWriter::default();
        replay.push(&encode(&ServerMessage::Welcome(game.welcome(None))));
        let connections = config.players.iter().map(|_| None).collect();
        Self {
            config,
            state: Mutex::new(State {
                game,
                connections,
                replay: Some(replay),
                socket,
                messages: 0,
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub async fn run(self: Arc<Self>) {
        let mut ticker = interval(Duration::from_secs(1) / TICK_RATE);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if let Some(finished) = self.step() {
                info!(
                    replay_bytes = finished.replay.len(),
                    scoreboard = finished.scoreboard,
                    "reporting the match"
                );
                tokio::spawn(replay::report(
                    self.config.replay_url.clone(),
                    finished.replay,
                    self.config.scoreboard_url.clone(),
                    finished.scoreboard,
                ));
            }
        }
    }

    fn step(&self) -> Option<Finished> {
        let mut state = self.state();
        for slot in 0..state.connections.len() {
            if state.connections[slot]
                .as_ref()
                .is_some_and(|c| c.heard.elapsed() > CLIENT_TIMEOUT)
            {
                state.drop_connection(slot);
                info!(slot, team = self.config.players[slot].team_id, "player went quiet");
            }
        }

        let was_over = state.game.phase() == Phase::Over;
        state.game.step();
        let just_ended = !was_over && state.game.phase() == Phase::Over;

        if just_ended || state.game.tick().is_multiple_of(SNAPSHOT_INTERVAL_TICKS) {
            let snapshot = ServerMessage::Snapshot(state.game.snapshot());
            state.broadcast(&snapshot);
        }
        let result = state.game.result().cloned()?;
        if just_ended || state.game.tick().is_multiple_of(GAME_OVER_INTERVAL_TICKS) {
            state.broadcast(&ServerMessage::GameOver(result.clone()));
        }
        if !just_ended {
            return None;
        }

        info!(
            winner = result.winner.map(|slot| state.game.players()[slot as usize].info.team_id.clone()),
            reason = ?result.reason,
            "match over"
        );
        let scoreboard = match state.game.scoreboard() {
            Some(scores) => json!({ "scores": scores.into_iter().collect::<BTreeMap<_, _>>() }),
            None => json!({ "void": true, "reason": "fewer than two teams connected" }),
        };
        Some(Finished {
            replay: state.replay.take()?.into_bytes(),
            scoreboard: scoreboard.to_string(),
        })
    }

    pub fn receive(&self, packet: &[u8], from: SocketAddr) {
        let mut state = self.state();
        let slot = state
            .connections
            .iter()
            .position(|c| c.as_ref().is_some_and(|c| c.addr == from));
        match (decode(packet), slot) {
            (Some(ClientMessage::Hello { version, key }), _) => self.hello(&mut state, from, version, &key),
            (Some(ClientMessage::Input(controls)), Some(slot)) => self.input(&mut state, slot, controls),
            _ => {}
        }
    }

    fn hello(&self, state: &mut State, from: SocketAddr, version: u16, key: &str) {
        if version != PROTOCOL_VERSION {
            return state.reject(from, "unsupported protocol version");
        }
        let Some(slot) = self.config.players.iter().position(|p| p.key == key) else {
            return state.reject(from, "unknown player key");
        };

        if state.connections[slot].as_ref().is_none_or(|c| c.addr != from) {
            state.kick(slot, "connected from somewhere else");
            let now = Instant::now();
            state.connections[slot] = Some(Connection {
                addr: from,
                heard: now,
                window: now,
                inputs: 0,
            });
            state.game.connect(slot);
            info!(slot, team = self.config.players[slot].team_id, peer = %from, "player connected");
        }
        let welcome = state.game.welcome(Some(slot as u8));
        state.send(slot, &ServerMessage::Welcome(welcome));
    }

    fn input(&self, state: &mut State, slot: usize, controls: Controls) {
        let Some(connection) = &mut state.connections[slot] else {
            return;
        };
        let now = Instant::now();
        connection.heard = now;
        if now.duration_since(connection.window) >= Duration::from_secs(1) {
            connection.window = now;
            connection.inputs = 0;
        }
        connection.inputs += 1;
        if connection.inputs > MAX_INPUTS_PER_SECOND {
            warn!(
                slot,
                team = self.config.players[slot].team_id,
                "kicking a player that floods inputs"
            );
            return state.kick(slot, "too many inputs");
        }
        state.game.set_controls(slot, controls);
    }
}

impl State {
    fn drop_connection(&mut self, slot: usize) {
        self.connections[slot] = None;
        self.game.disconnect(slot);
    }

    fn deliver(&mut self, frame: &[u8], to: impl IntoIterator<Item = SocketAddr>) {
        self.messages += 1;
        let datagrams = fragment(self.messages, frame);
        for addr in to {
            for datagram in &datagrams {
                let _ = self.socket.try_send_to(datagram, addr);
            }
        }
    }

    fn send(&mut self, slot: usize, message: &ServerMessage) {
        if let Some(addr) = self.connections[slot].as_ref().map(|c| c.addr) {
            self.deliver(&encode(message), [addr]);
        }
    }

    fn reject(&mut self, addr: SocketAddr, reason: &str) {
        self.deliver(&encode(&ServerMessage::Kick { reason: reason.into() }), [addr]);
    }

    fn kick(&mut self, slot: usize, reason: &str) {
        if let Some(addr) = self.connections[slot].as_ref().map(|c| c.addr) {
            self.reject(addr, reason);
            self.drop_connection(slot);
        }
    }

    fn broadcast(&mut self, message: &ServerMessage) {
        let frame = encode(message);
        if let Some(replay) = &mut self.replay {
            replay.push(&frame);
        }
        let connected: Vec<SocketAddr> = self.connections.iter().flatten().map(|c| c.addr).collect();
        self.deliver(&frame, connected);
    }
}
