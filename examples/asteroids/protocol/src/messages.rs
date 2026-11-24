use glam::Vec2;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Controls {
    // 1 turns left, -1 turns right
    pub turn: i8,
    pub thrust: bool,
    pub fire: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello { version: u16, key: String },
    Input(Controls),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    Welcome(Welcome),
    Snapshot(Snapshot),
    GameOver(GameOver),
    Kick { reason: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    // None for spectators
    pub slot: Option<u8>,
    pub players: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Lobby,
    Countdown,
    Playing,
    Over,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub score: u32,
    pub respawn_s: u8,
    pub connected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum EntityKind {
    Ship { slot: u8, thrusting: bool, shielded: bool },
    Asteroid { size: u8 },
    Bullet { owner: u8 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: u32,
    pub kind: EntityKind,
    pub position: Vec2,
    pub velocity: Vec2,
    pub angle: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    AsteroidDestroyed {
        position: Vec2,
        size: u8,
    },
    ShipDestroyed {
        position: Vec2,
        victim: u8,
        killer: Option<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u32,
    pub phase: Phase,
    pub phase_ms: u32,
    pub players: Vec<PlayerState>,
    pub entities: Vec<Entity>,
    pub events: Vec<Event>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    TimeUp,
    NoPlayers,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Standing {
    pub slot: u8,
    pub score: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameOver {
    pub reason: EndReason,
    pub winner: Option<u8>,
    pub standings: Vec<Standing>,
}

pub fn encode<T: Serialize>(message: &T) -> Vec<u8> {
    postcard::to_allocvec(message).expect("messages always serialize")
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}
