use std::collections::HashMap;
use std::f32::consts::TAU;
use std::time::Instant;

use asteroids_protocol::{
    EntityKind, Event, GameOver, Phase, SNAPSHOT_INTERVAL_TICKS, ServerMessage, Snapshot, TICK_RATE, Welcome,
    wrap_delta, wrap_position,
};
use bevy::prelude::*;

use crate::palette;

const SNAPSHOT_SECONDS: f32 = SNAPSHOT_INTERVAL_TICKS as f32 / TICK_RATE as f32;
const FEED_SECONDS: f32 = 5.0;

pub struct Frame {
    pub snapshot: Snapshot,
    at: Instant,
    index: HashMap<u32, usize>,
}

impl Frame {
    fn new(snapshot: Snapshot, at: Instant) -> Self {
        let index = snapshot.entities.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        Self { snapshot, at, index }
    }
}

pub struct Visible {
    pub id: u32,
    pub kind: EntityKind,
    pub position: Vec2,
    pub velocity: Vec2,
    pub heading: f32,
}

pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub length: f32,
    pub age: f32,
    pub ttl: f32,
    pub color: LinearRgba,
}

pub struct FeedLine {
    pub text: String,
    pub color: LinearRgba,
    pub age: f32,
}

#[derive(Resource, Default)]
pub struct View {
    pub welcome: Option<Welcome>,
    pub previous: Option<Frame>,
    pub current: Option<Frame>,
    pub game_over: Option<GameOver>,
    pub kicked: Option<String>,
    pub closed: Option<String>,
    pub rtt_ms: Option<u32>,
    pub particles: Vec<Particle>,
    pub feed: Vec<FeedLine>,
    playing_since: Option<Instant>,
}

fn lerp_angle(from: f32, to: f32, t: f32) -> f32 {
    let delta = (to - from + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0;
    from + delta * t
}

impl View {
    pub fn apply(&mut self, message: ServerMessage, now: Instant, effects: bool) {
        match message {
            ServerMessage::Welcome(welcome) => self.welcome = Some(welcome),
            ServerMessage::Snapshot(snapshot) => {
                if self.current.as_ref().is_some_and(|c| snapshot.tick <= c.snapshot.tick) {
                    return;
                }
                if effects {
                    self.spawn_effects(&snapshot.events);
                }
                if snapshot.phase == Phase::Playing {
                    self.playing_since.get_or_insert(now);
                }
                self.previous = self.current.take();
                self.current = Some(Frame::new(snapshot, now));
            }
            ServerMessage::GameOver(game_over) => self.game_over = Some(game_over),
            ServerMessage::Kick { reason } => self.kicked = Some(reason),
        }
    }

    pub fn rewind(&mut self) {
        self.previous = None;
        self.current = None;
        self.game_over = None;
        self.particles.clear();
        self.feed.clear();
        self.playing_since = None;
    }

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.current.as_ref().map(|f| &f.snapshot)
    }

    pub fn just_started(&self, now: Instant) -> bool {
        self.playing_since
            .is_some_and(|since| now.duration_since(since).as_secs_f32() < 1.2)
    }

    pub fn name(&self, slot: u8) -> String {
        let name = self
            .welcome
            .as_ref()
            .and_then(|w| w.players.get(slot as usize))
            .map_or("???", String::as_str);
        name.chars()
            .map(|c| {
                if (' '..='~').contains(&c) {
                    c.to_ascii_uppercase()
                } else {
                    '?'
                }
            })
            .take(18)
            .collect()
    }

    pub fn visible(&self, now: Instant) -> Vec<Visible> {
        let Some(current) = &self.current else {
            return Vec::new();
        };
        let alpha = (now.duration_since(current.at).as_secs_f32() / SNAPSHOT_SECONDS).clamp(0.0, 1.0);
        current
            .snapshot
            .entities
            .iter()
            .map(|entity| {
                let before = self
                    .previous
                    .as_ref()
                    .and_then(|p| p.index.get(&entity.id).map(|&i| &p.snapshot.entities[i]));
                let (position, heading) = match before {
                    Some(before) => (
                        wrap_position(before.position + wrap_delta(entity.position - before.position) * alpha),
                        lerp_angle(before.angle, entity.angle, alpha),
                    ),
                    None => (entity.position, entity.angle),
                };
                Visible {
                    id: entity.id,
                    kind: entity.kind,
                    position,
                    velocity: entity.velocity,
                    heading,
                }
            })
            .collect()
    }

    pub fn tick_effects(&mut self, dt: f32) {
        self.particles.retain_mut(|p| {
            p.age += dt;
            p.position += p.velocity * dt;
            p.velocity *= 1.0 - dt * 1.5;
            p.age < p.ttl
        });
        self.feed.retain_mut(|line| {
            line.age += dt;
            line.age < FEED_SECONDS
        });
    }

    fn burst(&mut self, at: Vec2, count: usize, speed: (f32, f32), ttl: (f32, f32), length: f32, color: LinearRgba) {
        for _ in 0..count {
            let direction = Vec2::from_angle(rand::random_range(0.0..TAU));
            self.particles.push(Particle {
                position: at,
                velocity: direction * rand::random_range(speed.0..speed.1),
                length,
                age: 0.0,
                ttl: rand::random_range(ttl.0..ttl.1),
                color,
            });
        }
    }

    fn spawn_effects(&mut self, events: &[Event]) {
        for event in events {
            match *event {
                Event::AsteroidDestroyed { position, size } => {
                    let count = 6 + size as usize * 5;
                    self.burst(position, count, (30.0, 150.0), (0.4, 1.0), 4.0, palette::ASTEROID);
                }
                Event::ShipDestroyed {
                    position,
                    victim,
                    killer,
                } => {
                    let color = palette::slot(victim);
                    self.burst(position, 36, (40.0, 220.0), (0.6, 1.6), 9.0, color);
                    self.burst(position, 12, (10.0, 60.0), (1.2, 2.2), 14.0, palette::TEXT);
                    let victim_name = self.name(victim);
                    let text = match killer {
                        Some(killer) => format!("{} DESTROYED {}", self.name(killer), victim_name),
                        None => format!("{victim_name} CRASHED"),
                    };
                    let color = killer.map(palette::slot).unwrap_or(palette::DIM_TEXT);
                    self.feed.push(FeedLine { text, color, age: 0.0 });
                }
            }
        }
    }
}
