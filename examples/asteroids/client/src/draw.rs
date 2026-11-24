use std::f32::consts::TAU;
use std::time::Instant;

use asteroids_protocol::{EndReason, EntityKind, Phase, WORLD_SIZE};
use bevy::color::Alpha;
use bevy::math::Isometry2d;
use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::palette;
use crate::session::Session;
use crate::view::View;

pub const HALF: Vec2 = Vec2::new(WORLD_SIZE.x / 2.0, WORLD_SIZE.y / 2.0);
pub const TOP_BAR: f32 = 84.0;
pub const BOTTOM_BAR: f32 = 44.0;
pub const MARGIN: f32 = 12.0;

const SHIP: [Vec2; 4] = [
    Vec2::new(15.0, 0.0),
    Vec2::new(-11.0, 9.0),
    Vec2::new(-6.0, 0.0),
    Vec2::new(-11.0, -9.0),
];
const ASTEROID_RADIUS: [f32; 4] = [0.0, 14.0, 28.0, 52.0];
const ASTEROID_VERTICES: usize = 11;
const STARS: usize = 160;

fn clip(a: Vec2, b: Vec2) -> Option<(Vec2, Vec2)> {
    let delta = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-delta.x, a.x + HALF.x),
        (delta.x, HALF.x - a.x),
        (-delta.y, a.y + HALF.y),
        (delta.y, HALF.y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            t0 = t0.max(r);
        } else {
            t1 = t1.min(r);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((a + delta * t0, a + delta * t1))
}

fn wrapped(position: Vec2, radius: f32) -> impl Iterator<Item = Vec2> {
    let axis = |value: f32, half: f32| -> [Option<f32>; 3] {
        [
            Some(0.0),
            (value + radius > half).then_some(-2.0 * half),
            (value - radius < -half).then_some(2.0 * half),
        ]
    };
    let xs = axis(position.x, HALF.x);
    let ys = axis(position.y, HALF.y);
    xs.into_iter()
        .flatten()
        .flat_map(move |x| ys.into_iter().flatten().map(move |y| position + Vec2::new(x, y)))
}

fn outline(gizmos: &mut Gizmos, points: &[Vec2], closed: bool, position: Vec2, radius: f32, color: LinearRgba) {
    let segments = if closed { points.len() } else { points.len() - 1 };
    for copy in wrapped(position, radius) {
        let shift = copy - position;
        for i in 0..segments {
            if let Some((a, b)) = clip(points[i] + shift, points[(i + 1) % points.len()] + shift) {
                gizmos.line_2d(a, b, color);
            }
        }
    }
}

fn transform(shape: &[Vec2], position: Vec2, heading: f32, scale: f32) -> Vec<Vec2> {
    let rotation = Vec2::from_angle(heading);
    shape.iter().map(|p| position + rotation.rotate(*p * scale)).collect()
}

fn asteroid_shape(id: u32, size: u8) -> Vec<Vec2> {
    let radius = ASTEROID_RADIUS[size.min(3) as usize];
    let mut rng = StdRng::seed_from_u64(id as u64);
    (0..ASTEROID_VERTICES)
        .map(|i| {
            let angle = i as f32 / ASTEROID_VERTICES as f32 * TAU;
            Vec2::from_angle(angle) * radius * rng.random_range(0.72..1.0)
        })
        .collect()
}

fn text(gizmos: &mut Gizmos, at: Vec2, content: &str, size: f32, anchor: Vec2, color: LinearRgba) {
    gizmos.text_2d(Isometry2d::from_translation(at), content, size, anchor, color);
}

const LEFT: Vec2 = Vec2::new(-0.5, 0.0);
const CENTER: Vec2 = Vec2::ZERO;
const RIGHT: Vec2 = Vec2::new(0.5, 0.0);

fn clock(ms: u32) -> String {
    let seconds = ms.div_ceil(1000);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn world(mut gizmos: Gizmos, view: Res<View>, time: Res<Time<Real>>) {
    gizmos.rect_2d(Isometry2d::IDENTITY, HALF * 2.0, palette::BORDER);
    let mut sky = StdRng::seed_from_u64(0);
    for _ in 0..STARS {
        let at = Vec2::new(sky.random_range(-HALF.x..HALF.x), sky.random_range(-HALF.y..HALF.y));
        gizmos.line_2d(at, at + Vec2::new(1.5, 0.0), palette::STAR);
    }

    let now = Instant::now();
    let blink = ((time.elapsed_secs() * 10.0) as u32).is_multiple_of(2);
    let own_slot = view.welcome.as_ref().and_then(|w| w.slot);

    for entity in view.visible(now) {
        match entity.kind {
            EntityKind::Asteroid { size } => {
                let shape = transform(&asteroid_shape(entity.id, size), entity.position, entity.heading, 1.0);
                outline(
                    &mut gizmos,
                    &shape,
                    true,
                    entity.position,
                    ASTEROID_RADIUS[size.min(3) as usize],
                    palette::ASTEROID,
                );
            }
            EntityKind::Ship {
                slot,
                thrusting,
                shielded,
            } => {
                let color = palette::slot(slot);
                let shape = transform(&SHIP, entity.position, entity.heading, 1.0);
                outline(&mut gizmos, &shape, true, entity.position, 16.0, color);
                if thrusting {
                    let flicker = rand::random_range(16.0..22.0);
                    let flame = [Vec2::new(-8.0, 4.5), Vec2::new(-flicker, 0.0), Vec2::new(-8.0, -4.5)];
                    let flame = transform(&flame, entity.position, entity.heading, 1.0);
                    outline(&mut gizmos, &flame, false, entity.position, 24.0, palette::FLAME);
                }
                if shielded && blink {
                    gizmos
                        .circle_2d(entity.position, 21.0, color.with_alpha(0.6))
                        .resolution(28);
                }
                let label = if Some(slot) == own_slot {
                    "YOU".to_string()
                } else {
                    view.name(slot)
                };
                text(
                    &mut gizmos,
                    entity.position + Vec2::new(0.0, 28.0),
                    &label,
                    11.0,
                    CENTER,
                    color.with_alpha(0.7),
                );
            }
            EntityKind::Bullet { owner } => {
                let tail = entity.velocity.normalize_or_zero() * 7.0;
                if let Some((a, b)) = clip(entity.position - tail, entity.position) {
                    gizmos.line_2d(a, b, palette::slot(owner));
                }
            }
        }
    }

    for particle in &view.particles {
        let fade = 1.0 - particle.age / particle.ttl;
        let tail = particle.velocity.normalize_or_zero() * particle.length * fade.max(0.2);
        if let Some((a, b)) = clip(particle.position - tail, particle.position) {
            gizmos.line_2d(a, b, particle.color.with_alpha(fade));
        }
    }
}

pub fn hud(mut gizmos: Gizmos, view: Res<View>, session: Res<Session>) {
    let now = Instant::now();
    let top = HALF.y + TOP_BAR / 2.0;
    let bottom = -HALF.y - BOTTOM_BAR / 2.0;
    let own_slot = view.welcome.as_ref().and_then(|w| w.slot);

    if let (Some(welcome), Some(snapshot)) = (&view.welcome, view.snapshot()) {
        let columns = welcome.players.len().max(1) as f32;
        let width = HALF.x * 2.0 / columns;
        for (slot, player) in snapshot.players.iter().enumerate() {
            let slot = slot as u8;
            let color = palette::slot(slot);
            let dim = if player.connected || snapshot.phase == Phase::Over {
                1.0
            } else {
                0.35
            };
            let x = -HALF.x + width * slot as f32 + 6.0;
            let mut name = view.name(slot);
            if Some(slot) == own_slot {
                name.push_str(" (YOU)");
            }
            if !player.connected && snapshot.phase != Phase::Over {
                name.push_str(" - OFFLINE");
            }
            text(
                &mut gizmos,
                Vec2::new(x, top + 16.0),
                &name,
                17.0,
                LEFT,
                color.with_alpha(dim),
            );
            text(
                &mut gizmos,
                Vec2::new(x, top - 14.0),
                &format!("{:>6}", player.score),
                22.0,
                LEFT,
                palette::TEXT.with_alpha(dim),
            );
            if player.respawn_s > 0 {
                text(
                    &mut gizmos,
                    Vec2::new(x + 150.0, top - 14.0),
                    &format!("OUT {}", player.respawn_s),
                    17.0,
                    LEFT,
                    palette::DIM_TEXT.with_alpha(dim),
                );
            }
        }

        let status = match snapshot.phase {
            Phase::Lobby => "LOBBY".to_string(),
            Phase::Countdown => "GET READY".to_string(),
            Phase::Playing => clock(snapshot.phase_ms),
            Phase::Over => "MATCH OVER".to_string(),
        };
        text(
            &mut gizmos,
            Vec2::new(0.0, bottom),
            &status,
            20.0,
            CENTER,
            palette::TEXT,
        );
    }

    let connection = match &*session {
        Session::Replay(replay) => replay.describe(),
        Session::Live(_) => match (&view.closed, view.rtt_ms) {
            (Some(_), _) => "OFFLINE".to_string(),
            (None, Some(rtt)) => format!("PING {rtt}MS"),
            (None, None) if view.welcome.is_some() => "ONLINE".to_string(),
            (None, None) => "CONNECTING".to_string(),
        },
    };
    text(
        &mut gizmos,
        Vec2::new(-HALF.x, bottom),
        &connection,
        13.0,
        LEFT,
        palette::DIM_TEXT,
    );
    let hint = match &*session {
        Session::Replay(_) => "SPACE PAUSE   LEFT/RIGHT SEEK   UP/DOWN SPEED",
        Session::Live(_) => "LEFT/RIGHT TURN   UP THRUST   SPACE FIRE",
    };
    text(
        &mut gizmos,
        Vec2::new(HALF.x, bottom),
        hint,
        13.0,
        RIGHT,
        palette::DIM_TEXT,
    );

    for (i, line) in view.feed.iter().enumerate() {
        let fade = (1.0 - line.age / 5.0).clamp(0.0, 1.0) * 2.0;
        let at = Vec2::new(HALF.x - 12.0, HALF.y - 20.0 - i as f32 * 20.0);
        text(
            &mut gizmos,
            at,
            &line.text,
            12.0,
            RIGHT,
            line.color.with_alpha(fade.min(1.0)),
        );
    }

    centre_overlay(&mut gizmos, &view, now);
}

fn centre_overlay(gizmos: &mut Gizmos, view: &View, now: Instant) {
    let big = |gizmos: &mut Gizmos, y: f32, content: &str, size: f32, color: LinearRgba| {
        text(gizmos, Vec2::new(0.0, y), content, size, CENTER, color);
    };

    if let Some(game_over) = &view.game_over {
        big(gizmos, 230.0, "GAME OVER", 64.0, palette::TEXT);
        let headline = match (game_over.reason, game_over.winner) {
            (EndReason::NoPlayers, _) => "NOT ENOUGH PLAYERS".to_string(),
            (_, Some(winner)) => format!("{} WINS", view.name(winner)),
            (_, None) => "DRAW".to_string(),
        };
        let color = game_over.winner.map(palette::slot).unwrap_or(palette::TEXT);
        big(gizmos, 160.0, &headline, 34.0, color);
        for (row, standing) in game_over.standings.iter().enumerate() {
            let line = format!("{}. {:<18} {:>6}", row + 1, view.name(standing.slot), standing.score);
            big(
                gizmos,
                90.0 - row as f32 * 32.0,
                &line,
                18.0,
                palette::slot(standing.slot),
            );
        }
        if let Some(reason) = &view.kicked {
            big(gizmos, -260.0, &reason.to_uppercase(), 16.0, palette::DIM_TEXT);
        }
        return;
    }

    if let Some(reason) = view.kicked.as_ref().or(view.closed.as_ref()) {
        big(gizmos, 40.0, "DISCONNECTED", 48.0, palette::TEXT);
        big(gizmos, -20.0, &reason.to_uppercase(), 16.0, palette::DIM_TEXT);
        return;
    }

    let Some(snapshot) = view.snapshot() else {
        big(gizmos, 0.0, "CONNECTING", 40.0, palette::TEXT);
        return;
    };
    match snapshot.phase {
        Phase::Lobby => {
            let connected = snapshot.players.iter().filter(|p| p.connected).count();
            let headline = format!("WAITING FOR PLAYERS  {connected}/{}", snapshot.players.len());
            big(gizmos, 40.0, &headline, 36.0, palette::TEXT);
            let deadline = format!("THE MATCH STARTS IN {} AT THE LATEST", clock(snapshot.phase_ms));
            big(gizmos, -10.0, &deadline, 16.0, palette::DIM_TEXT);
        }
        Phase::Countdown => {
            big(
                gizmos,
                0.0,
                &snapshot.phase_ms.div_ceil(1000).to_string(),
                120.0,
                palette::TEXT,
            );
        }
        Phase::Playing => {
            if view.just_started(now) {
                big(gizmos, 0.0, "GO!", 96.0, palette::TEXT);
            }
            let own = view.welcome.as_ref().and_then(|w| w.slot);
            if let Some((own, state)) = own.and_then(|slot| Some((slot, snapshot.players.get(slot as usize)?))) {
                let alive = snapshot
                    .entities
                    .iter()
                    .any(|e| matches!(e.kind, EntityKind::Ship { slot, .. } if slot == own));
                if state.respawn_s > 0 {
                    let countdown = format!("BACK IN {}", state.respawn_s);
                    big(gizmos, -60.0, &countdown, 32.0, palette::DIM_TEXT);
                } else if !alive {
                    big(gizmos, -60.0, "TOO LATE TO JOIN", 32.0, palette::DIM_TEXT);
                }
            }
        }
        Phase::Over => {}
    }
}
