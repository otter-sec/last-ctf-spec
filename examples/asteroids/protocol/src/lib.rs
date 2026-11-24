mod datagram;
mod messages;
mod replay;

pub use datagram::{Reassembler, fragment};
pub use glam::Vec2;
pub use messages::*;
pub use replay::{ReplayWriter, read_replay};

pub const PROTOCOL_VERSION: u16 = 6;

pub const TICK_RATE: u32 = 60;
pub const SNAPSHOT_INTERVAL_TICKS: u32 = 2;
pub const MAX_PLAYERS: usize = 4;

pub const WORLD_SIZE: Vec2 = Vec2::new(1600.0, 1000.0);

pub fn wrap_position(position: Vec2) -> Vec2 {
    let half = WORLD_SIZE * 0.5;
    (position + half).rem_euclid(WORLD_SIZE) - half
}

pub fn wrap_delta(delta: Vec2) -> Vec2 {
    delta - WORLD_SIZE * (delta / WORLD_SIZE).round()
}
