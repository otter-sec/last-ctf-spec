use std::path::Path;
use std::time::Instant;

use asteroids_protocol::{SNAPSHOT_INTERVAL_TICKS, ServerMessage, TICK_RATE, read_replay};
use bevy::prelude::*;

use crate::net::{Connection, NetEvent};
use crate::view::View;

const SEEK_SECONDS: f32 = 5.0;

#[derive(Resource)]
pub enum Session {
    Live(Connection),
    Replay(ReplayPlayer),
}

pub struct ReplayPlayer {
    frames: Vec<(u32, ServerMessage)>,
    cursor: usize,
    first_tick: u32,
    last_tick: u32,
    clock: f32,
    speed: f32,
    paused: bool,
}

impl ReplayPlayer {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|err| format!("couldn't read {}: {err}", path.display()))?;
        let messages = read_replay(&bytes).ok_or_else(|| format!("{} is not a replay", path.display()))?;

        let mut tick = 0;
        let frames: Vec<_> = messages
            .into_iter()
            .map(|message| {
                if let ServerMessage::Snapshot(snapshot) = &message {
                    tick = snapshot.tick;
                }
                (tick, message)
            })
            .collect();
        let snapshots = || frames.iter().filter(|(_, m)| matches!(m, ServerMessage::Snapshot(_)));
        let first_tick = snapshots()
            .next()
            .map(|(tick, _)| *tick)
            .ok_or("the replay has no snapshots")?;
        let last_tick = snapshots().next_back().map(|(tick, _)| *tick).unwrap_or(first_tick);
        Ok(Self {
            frames,
            cursor: 0,
            first_tick,
            last_tick,
            clock: 0.0,
            speed: 1.0,
            paused: false,
        })
    }

    fn target_tick(&self) -> u32 {
        self.first_tick + (self.clock * TICK_RATE as f32) as u32
    }

    fn duration(&self) -> f32 {
        (self.last_tick - self.first_tick) as f32 / TICK_RATE as f32
    }

    pub fn describe(&self) -> String {
        let at = self.clock.min(self.duration()) as u32;
        let total = self.duration() as u32;
        let state = if self.paused { "PAUSED" } else { "REPLAY" };
        format!(
            "{state} {}:{:02} / {}:{:02}  {}X",
            at / 60,
            at % 60,
            total / 60,
            total % 60,
            self.speed
        )
    }

    fn advance(&mut self, view: &mut View, dt: f32, keys: &ButtonInput<KeyCode>) {
        if keys.just_pressed(KeyCode::Space) {
            self.paused = !self.paused;
        }
        if keys.just_pressed(KeyCode::ArrowUp) {
            self.speed = (self.speed * 2.0).min(8.0);
        }
        if keys.just_pressed(KeyCode::ArrowDown) {
            self.speed = (self.speed / 2.0).max(0.25);
        }

        let mut seek = 0.0;
        if keys.just_pressed(KeyCode::ArrowRight) {
            seek += SEEK_SECONDS;
        }
        if keys.just_pressed(KeyCode::ArrowLeft) {
            seek -= SEEK_SECONDS;
        }
        if !self.paused {
            self.clock += dt * self.speed;
        }
        self.clock = (self.clock + seek).clamp(0.0, self.duration() + 1.0);

        let now = Instant::now();
        let target = self.target_tick();
        if seek < 0.0 {
            view.rewind();
            let from = target.saturating_sub(SNAPSHOT_INTERVAL_TICKS * 2);
            self.cursor = self.frames.partition_point(|(tick, _)| *tick < from);
        }
        while let Some((tick, message)) = self.frames.get(self.cursor) {
            if *tick > target {
                break;
            }
            view.apply(message.clone(), now, seek == 0.0);
            self.cursor += 1;
        }
    }
}

pub fn pump(
    mut session: ResMut<Session>,
    mut view: ResMut<View>,
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let now = Instant::now();
    match &mut *session {
        Session::Live(connection) => {
            for event in connection.poll() {
                match event {
                    NetEvent::Message(message) => view.apply(message, now, true),
                    NetEvent::Rtt(rtt) => view.rtt_ms = Some(rtt),
                    NetEvent::Closed(reason) => {
                        view.closed.get_or_insert(reason);
                    }
                }
            }
        }
        Session::Replay(replay) => replay.advance(&mut view, time.delta_secs(), &keys),
    }
    view.tick_effects(time.delta_secs());
}
