mod draw;
mod net;
mod palette;
mod session;
mod view;

use std::path::PathBuf;

use asteroids_protocol::{Controls, TICK_RATE};
use bevy::camera::ScalingMode;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use clap::Parser;

use crate::draw::{BOTTOM_BAR, HALF, MARGIN, TOP_BAR};
use crate::net::Connection;
use crate::session::{ReplayPlayer, Session};
use crate::view::View;

const INPUT_RATE: f32 = TICK_RATE as f32 / 2.0;

#[derive(Parser)]
#[command(about = "Asteroids, but it's a match against other teams")]
struct Args {
    #[arg(
        long,
        required_unless_present = "replay",
        help = "game server, as shown on the challenge page, or udp://host:port for one you run yourself"
    )]
    server: Option<String>,
    #[arg(
        long,
        env = "ASTEROIDS_KEY",
        required_unless_present = "replay",
        help = "your player key from the challenge page"
    )]
    key: Option<String>,
    #[arg(long, conflicts_with_all = ["server", "key"], help = "watch a replay file instead of playing")]
    replay: Option<PathBuf>,
}

#[derive(Resource, Default)]
struct Pilot {
    pending: f32,
}

fn main() -> AppExit {
    let args = Args::parse();
    let session = match (args.replay, args.server, args.key) {
        (Some(path), _, _) => ReplayPlayer::load(&path).map(Session::Replay),
        (None, Some(server), Some(key)) => Ok(Session::Live(Connection::open(server, key))),
        _ => Err("--server and --key are required".into()),
    };
    let session = match session {
        Ok(session) => session,
        Err(err) => {
            eprintln!("{err}");
            return AppExit::error();
        }
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "asteroids".into(),
                resolution: (1300, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(session)
        .init_resource::<View>()
        .init_resource::<Pilot>()
        .add_systems(Startup, setup)
        .add_systems(Update, (session::pump, fly, draw::world, draw::hud).chain())
        .run()
}

fn setup(mut commands: Commands, mut gizmos: ResMut<GizmoConfigStore>) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: HALF.x * 2.0 + MARGIN * 2.0,
                min_height: HALF.y * 2.0 + TOP_BAR + BOTTOM_BAR,
            },
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(0.0, (TOP_BAR - BOTTOM_BAR) / 2.0, 0.0),
        Tonemapping::AcesFitted,
        Bloom::NATURAL,
    ));
    let (config, _) = gizmos.config_mut::<DefaultGizmoConfigGroup>();
    config.line.width = 2.0;
}

fn fly(session: Res<Session>, mut pilot: ResMut<Pilot>, keys: Res<ButtonInput<KeyCode>>, time: Res<Time<Real>>) {
    let Session::Live(connection) = &*session else {
        return;
    };
    pilot.pending += time.delta_secs();
    if pilot.pending < 1.0 / INPUT_RATE {
        return;
    }
    pilot.pending = 0.0;

    let held = |a: KeyCode, b: KeyCode| keys.pressed(a) || keys.pressed(b);
    connection.send(Controls {
        turn: held(KeyCode::ArrowLeft, KeyCode::KeyA) as i8 - held(KeyCode::ArrowRight, KeyCode::KeyD) as i8,
        thrust: held(KeyCode::ArrowUp, KeyCode::KeyW),
        fire: keys.pressed(KeyCode::Space),
    });
}
