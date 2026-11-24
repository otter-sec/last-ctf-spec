use std::cmp::{Ordering, Reverse};
use std::f32::consts::{FRAC_PI_2, TAU};

use asteroids_protocol::{
    Controls, EndReason, Entity, EntityKind, Event, GameOver, Phase, PlayerState, Snapshot, Standing, TICK_RATE, Vec2,
    WORLD_SIZE, Welcome, wrap_delta, wrap_position,
};
use rand::RngExt;
use rand::rngs::StdRng;

use crate::config::{Config, MatchPlayer};

const DT: f32 = 1.0 / TICK_RATE as f32;

const SHIP_RADIUS: f32 = 12.0;
const SHIP_NOSE: f32 = 14.0;
const THRUST: f32 = 320.0;
const TURN_RATE: f32 = 4.2;
const MAX_SPEED: f32 = 400.0;
const DRAG: f32 = 0.4;
const FIRE_COOLDOWN: f32 = 0.2;
const MAX_BULLETS_PER_SHIP: usize = 5;
const BULLET_SPEED: f32 = 640.0;
const BULLET_TTL: f32 = 1.1;
const BULLET_RADIUS: f32 = 2.0;
const SPAWN_SHIELD: f32 = 3.0;
const SPAWN_CLEARANCE: f32 = 160.0;

const ASTEROID_RADIUS: [f32; 4] = [0.0, 14.0, 28.0, 52.0];
const ASTEROID_SPEED: [(f32, f32); 4] = [(0.0, 0.0), (70.0, 130.0), (45.0, 90.0), (25.0, 55.0)];
const ASTEROID_SCORE: [u32; 4] = [0, 100, 50, 20];
const ASTEROID_MASS: [u32; 4] = [0, 1, 2, 4];
const INITIAL_ASTEROIDS: usize = 6;
const MIN_ASTEROID_MASS: u32 = 16;
const MAX_ASTEROIDS: usize = 40;
const ASTEROID_SPAWN_INTERVAL: f32 = 2.0;

const COUNTDOWN_TICKS: u32 = 3 * TICK_RATE;

#[derive(Clone, Copy)]
struct Ship {
    id: u32,
    position: Vec2,
    velocity: Vec2,
    angle: f32,
    cooldown: f32,
    shield: f32,
    thrusting: bool,
}

#[derive(Clone, Copy)]
struct Asteroid {
    id: u32,
    size: u8,
    position: Vec2,
    velocity: Vec2,
    angle: f32,
    spin: f32,
}

impl Asteroid {
    fn radius(&self) -> f32 {
        ASTEROID_RADIUS[self.size as usize]
    }
}

#[derive(Clone, Copy)]
struct Bullet {
    id: u32,
    owner: u8,
    position: Vec2,
    velocity: Vec2,
    ttl: f32,
}

pub struct Player {
    pub info: MatchPlayer,
    controls: Controls,
    connected: bool,
    // connected before the lobby closed
    playing: bool,
    ship: Option<Ship>,
    respawn_in: f32,
    score: u32,
}

impl Player {
    fn standing(&self) -> (bool, u32) {
        (self.playing, self.score)
    }
}

pub struct Game {
    tick: u32,
    phase: Phase,
    phase_ends: u32,
    respawn_delay: f32,
    match_ticks: u32,
    players: Vec<Player>,
    asteroids: Vec<Asteroid>,
    bullets: Vec<Bullet>,
    events: Vec<Event>,
    next_id: u32,
    asteroid_timer: f32,
    rng: StdRng,
    result: Option<GameOver>,
}

fn overlaps(a: Vec2, b: Vec2, distance: f32) -> bool {
    wrap_delta(a - b).length_squared() < distance * distance
}

fn seconds_to_ticks(seconds: f32) -> u32 {
    (seconds * TICK_RATE as f32).round() as u32
}

fn spawn_point(slot: usize) -> Vec2 {
    let corner = [(-1.0, 1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, -1.0)][slot % 4];
    Vec2::new(corner.0, corner.1) * WORLD_SIZE * 0.3
}

impl Game {
    pub fn new(config: &Config) -> Self {
        let mut game = Self {
            tick: 0,
            phase: Phase::Lobby,
            phase_ends: seconds_to_ticks(config.lobby_timeout.as_secs_f32()),
            respawn_delay: config.respawn_delay.as_secs_f32(),
            match_ticks: seconds_to_ticks(config.match_duration.as_secs_f32()),
            players: config
                .players
                .iter()
                .map(|info| Player {
                    info: info.clone(),
                    controls: Controls::default(),
                    connected: false,
                    playing: false,
                    ship: None,
                    respawn_in: 0.0,
                    score: 0,
                })
                .collect(),
            asteroids: Vec::new(),
            bullets: Vec::new(),
            events: Vec::new(),
            next_id: 1,
            asteroid_timer: 0.0,
            rng: rand::make_rng(),
            result: None,
        };

        let spawns: Vec<Vec2> = (0..game.players.len()).map(spawn_point).collect();
        for _ in 0..INITIAL_ASTEROIDS {
            let position = (0..32)
                .map(|_| game.random_point())
                .find(|p| spawns.iter().all(|s| !overlaps(*p, *s, 280.0)))
                .unwrap_or_else(|| game.random_point());
            let velocity = game.random_direction() * game.random_speed(3);
            game.spawn_asteroid(3, position, velocity);
        }
        game
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn tick(&self) -> u32 {
        self.tick
    }

    pub fn players(&self) -> &[Player] {
        &self.players
    }

    pub fn result(&self) -> Option<&GameOver> {
        self.result.as_ref()
    }

    fn playing(&self) -> usize {
        self.players.iter().filter(|p| p.playing).count()
    }

    // 2/(N-1) for every team finished ahead of and half of that for a tie, which adds up to N
    pub fn scoreboard(&self) -> Option<Vec<(String, f64)>> {
        if self.result.is_none() || self.playing() < 2 {
            return None;
        }
        let opponents = (self.players.len() - 1) as f64;
        Some(
            self.players
                .iter()
                .enumerate()
                .map(|(slot, player)| {
                    let beaten: f64 = self
                        .players
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other != slot)
                        .map(|(_, other)| match player.standing().cmp(&other.standing()) {
                            Ordering::Greater => 1.0,
                            Ordering::Equal => 0.5,
                            Ordering::Less => 0.0,
                        })
                        .sum();
                    (player.info.team_id.clone(), 2.0 * beaten / opponents)
                })
                .collect(),
        )
    }

    pub fn welcome(&self, slot: Option<u8>) -> Welcome {
        Welcome {
            slot,
            players: self.players.iter().map(|p| p.info.name.clone()).collect(),
        }
    }

    pub fn connect(&mut self, slot: usize) {
        let player = &mut self.players[slot];
        player.connected = true;
        if self.phase == Phase::Lobby {
            player.playing = true;
        }
    }

    pub fn disconnect(&mut self, slot: usize) {
        let player = &mut self.players[slot];
        player.connected = false;
        player.controls = Controls::default();
    }

    pub fn set_controls(&mut self, slot: usize, controls: Controls) {
        self.players[slot].controls = controls;
    }

    pub fn step(&mut self) {
        self.tick += 1;
        match self.phase {
            Phase::Lobby => {
                self.drift();
                let playing = self.playing();
                if playing == self.players.len() {
                    self.start_countdown();
                } else if self.tick >= self.phase_ends {
                    if playing < 2 {
                        self.finish(EndReason::NoPlayers);
                    } else {
                        self.start_countdown();
                    }
                }
            }
            Phase::Countdown => {
                self.drift();
                if self.tick >= self.phase_ends {
                    self.phase = Phase::Playing;
                    self.phase_ends = self.tick + self.match_ticks;
                }
            }
            Phase::Playing => {
                self.respawn_ships();
                self.fly_ships();
                self.drift_projectiles();
                self.resolve_collisions();
                self.replenish_asteroids();
                if self.tick >= self.phase_ends {
                    self.finish(EndReason::TimeUp);
                }
            }
            Phase::Over => self.drift(),
        }
    }

    pub fn snapshot(&mut self) -> Snapshot {
        let ships = self.players.iter().enumerate().filter_map(|(slot, player)| {
            let ship = player.ship?;
            Some(Entity {
                id: ship.id,
                kind: EntityKind::Ship {
                    slot: slot as u8,
                    thrusting: ship.thrusting,
                    shielded: ship.shield > 0.0,
                },
                position: ship.position,
                velocity: ship.velocity,
                angle: ship.angle,
            })
        });
        let asteroids = self.asteroids.iter().map(|a| Entity {
            id: a.id,
            kind: EntityKind::Asteroid { size: a.size },
            position: a.position,
            velocity: a.velocity,
            angle: a.angle,
        });
        let bullets = self.bullets.iter().map(|b| Entity {
            id: b.id,
            kind: EntityKind::Bullet { owner: b.owner },
            position: b.position,
            velocity: b.velocity,
            angle: 0.0,
        });
        let entities = ships.chain(asteroids).chain(bullets).collect();

        let playing = self.phase == Phase::Playing;
        Snapshot {
            tick: self.tick,
            phase: self.phase,
            phase_ms: self.phase_ends.saturating_sub(self.tick) * 1000 / TICK_RATE,
            players: self
                .players
                .iter()
                .map(|p| PlayerState {
                    score: p.score,
                    respawn_s: if playing && p.playing && p.ship.is_none() {
                        p.respawn_in.ceil().max(1.0) as u8
                    } else {
                        0
                    },
                    connected: p.connected,
                })
                .collect(),
            entities,
            events: std::mem::take(&mut self.events),
        }
    }

    fn start_countdown(&mut self) {
        for slot in 0..self.players.len() {
            if self.players[slot].playing {
                let ship = self.new_ship(spawn_point(slot), SPAWN_SHIELD + COUNTDOWN_TICKS as f32 * DT);
                self.players[slot].ship = Some(ship);
            }
        }
        self.phase = Phase::Countdown;
        self.phase_ends = self.tick + COUNTDOWN_TICKS;
    }

    fn finish(&mut self, reason: EndReason) {
        let mut order: Vec<usize> = (0..self.players.len()).collect();
        order.sort_by_key(|&slot| Reverse(self.players[slot].standing()));

        let standing = |place: usize| order.get(place).map(|&slot| self.players[slot].standing());
        let won = reason == EndReason::TimeUp && self.playing() >= 2 && standing(0) > standing(1);

        self.result = Some(GameOver {
            reason,
            winner: won.then(|| order[0] as u8),
            standings: order
                .iter()
                .map(|&slot| Standing {
                    slot: slot as u8,
                    score: self.players[slot].score,
                })
                .collect(),
        });
        self.phase = Phase::Over;
        self.phase_ends = self.tick;
        for player in &mut self.players {
            if let Some(ship) = &mut player.ship {
                ship.thrusting = false;
            }
        }
    }

    fn drift(&mut self) {
        self.drift_projectiles();
        for ship in self.players.iter_mut().filter_map(|p| p.ship.as_mut()) {
            ship.position = wrap_position(ship.position + ship.velocity * DT);
            ship.shield = (ship.shield - DT).max(0.0);
        }
    }

    fn respawn_ships(&mut self) {
        for slot in 0..self.players.len() {
            let player = &mut self.players[slot];
            if player.ship.is_some() || !player.playing {
                continue;
            }
            player.respawn_in -= DT;
            if player.respawn_in > 0.0 {
                continue;
            }
            let position = self.safest_spawn(slot);
            let ship = self.new_ship(position, SPAWN_SHIELD);
            self.players[slot].ship = Some(ship);
        }
    }

    fn fly_ships(&mut self) {
        let mut fired = Vec::new();
        for (slot, player) in self.players.iter_mut().enumerate() {
            let Some(ship) = &mut player.ship else {
                continue;
            };
            let controls = player.controls;

            ship.angle = (ship.angle + controls.turn.clamp(-1, 1) as f32 * TURN_RATE * DT).rem_euclid(TAU);
            let forward = Vec2::from_angle(ship.angle);
            if controls.thrust {
                ship.velocity += forward * THRUST * DT;
            }
            ship.velocity = (ship.velocity * (-DRAG * DT).exp()).clamp_length_max(MAX_SPEED);
            ship.position = wrap_position(ship.position + ship.velocity * DT);
            ship.thrusting = controls.thrust;
            ship.shield = (ship.shield - DT).max(0.0);
            ship.cooldown = (ship.cooldown - DT).max(0.0);

            if controls.fire && ship.cooldown <= 0.0 {
                ship.cooldown = FIRE_COOLDOWN;
                fired.push((
                    slot as u8,
                    ship.position + forward * SHIP_NOSE,
                    forward * BULLET_SPEED + ship.velocity,
                ));
            }
        }

        for (owner, position, velocity) in fired {
            if self.bullets.iter().filter(|b| b.owner == owner).count() >= MAX_BULLETS_PER_SHIP {
                continue;
            }
            let id = self.next_id();
            self.bullets.push(Bullet {
                id,
                owner,
                position: wrap_position(position),
                velocity,
                ttl: BULLET_TTL,
            });
        }
    }

    fn drift_projectiles(&mut self) {
        for asteroid in &mut self.asteroids {
            asteroid.position = wrap_position(asteroid.position + asteroid.velocity * DT);
            asteroid.angle = (asteroid.angle + asteroid.spin * DT).rem_euclid(TAU);
        }
        self.bullets.retain_mut(|bullet| {
            bullet.position = wrap_position(bullet.position + bullet.velocity * DT);
            bullet.ttl -= DT;
            bullet.ttl > 0.0
        });
    }

    fn resolve_collisions(&mut self) {
        let mut index = 0;
        while index < self.bullets.len() {
            let bullet = self.bullets[index];
            let asteroid = self
                .asteroids
                .iter()
                .position(|a| overlaps(bullet.position, a.position, a.radius() + BULLET_RADIUS));
            if let Some(asteroid) = asteroid {
                self.bullets.swap_remove(index);
                let asteroid = self.asteroids.swap_remove(asteroid);
                self.players[bullet.owner as usize].score += ASTEROID_SCORE[asteroid.size as usize];
                self.destroy_asteroid(asteroid, bullet.velocity);
                continue;
            }

            let victim = self.players.iter().enumerate().position(|(slot, p)| {
                slot as u8 != bullet.owner
                    && p.ship.is_some_and(|s| {
                        s.shield <= 0.0 && overlaps(bullet.position, s.position, SHIP_RADIUS + BULLET_RADIUS)
                    })
            });
            if let Some(victim) = victim {
                self.bullets.swap_remove(index);
                self.destroy_ship(victim, Some(bullet.owner));
                continue;
            }
            index += 1;
        }

        for slot in 0..self.players.len() {
            let Some(ship) = self.players[slot].ship.filter(|s| s.shield <= 0.0) else {
                continue;
            };
            let asteroid = self
                .asteroids
                .iter()
                .position(|a| overlaps(ship.position, a.position, a.radius() + SHIP_RADIUS));
            if let Some(asteroid) = asteroid {
                let asteroid = self.asteroids.swap_remove(asteroid);
                self.destroy_asteroid(asteroid, ship.velocity * 0.3);
                self.destroy_ship(slot, None);
            }
        }

        for a in 0..self.players.len() {
            for b in a + 1..self.players.len() {
                let (Some(first), Some(second)) = (self.players[a].ship, self.players[b].ship) else {
                    continue;
                };
                if first.shield <= 0.0
                    && second.shield <= 0.0
                    && overlaps(first.position, second.position, SHIP_RADIUS * 2.0)
                {
                    self.destroy_ship(a, None);
                    self.destroy_ship(b, None);
                }
            }
        }
    }

    fn destroy_ship(&mut self, slot: usize, killer: Option<u8>) {
        let player = &mut self.players[slot];
        let Some(ship) = player.ship.take() else {
            return;
        };
        player.respawn_in = self.respawn_delay;
        self.events.push(Event::ShipDestroyed {
            position: ship.position,
            victim: slot as u8,
            killer,
        });
    }

    fn destroy_asteroid(&mut self, asteroid: Asteroid, impulse: Vec2) {
        self.events.push(Event::AsteroidDestroyed {
            position: asteroid.position,
            size: asteroid.size,
        });
        if asteroid.size <= 1 {
            return;
        }

        let size = asteroid.size - 1;
        let heading = if asteroid.velocity.length_squared() > 1.0 {
            asteroid.velocity.normalize()
        } else {
            self.random_direction()
        };
        for side in [-1.0f32, 1.0] {
            let direction = Vec2::from_angle(side * self.rng.random_range(0.35..1.2)).rotate(heading);
            let offset = Vec2::from_angle(side * FRAC_PI_2).rotate(heading) * ASTEROID_RADIUS[size as usize];
            let velocity = direction * self.random_speed(size) + impulse * 0.06;
            self.spawn_asteroid(size, wrap_position(asteroid.position + offset), velocity);
        }
    }

    fn replenish_asteroids(&mut self) {
        self.asteroid_timer -= DT;
        let mass: u32 = self.asteroids.iter().map(|a| ASTEROID_MASS[a.size as usize]).sum();
        if mass >= MIN_ASTEROID_MASS || self.asteroids.len() >= MAX_ASTEROIDS || self.asteroid_timer > 0.0 {
            return;
        }
        self.asteroid_timer = ASTEROID_SPAWN_INTERVAL;

        let ships: Vec<Vec2> = self.players.iter().filter_map(|p| p.ship.map(|s| s.position)).collect();
        for _ in 0..8 {
            let along = self.rng.random_range(-0.5..0.5);
            let edge = if self.rng.random_bool(0.5) {
                Vec2::new(-0.5, along) * WORLD_SIZE
            } else {
                Vec2::new(along, -0.5) * WORLD_SIZE
            };
            if ships.iter().all(|s| !overlaps(edge, *s, SPAWN_CLEARANCE)) {
                let velocity = self.random_direction() * self.random_speed(3);
                self.spawn_asteroid(3, edge, velocity);
                return;
            }
        }
    }

    fn safest_spawn(&self, slot: usize) -> Vec2 {
        let mut candidates = vec![spawn_point(slot)];
        candidates.extend((0..4).map(spawn_point));
        candidates
            .extend([(0.0, 0.35), (0.0, -0.35), (0.35, 0.0), (-0.35, 0.0)].map(|(x, y)| Vec2::new(x, y) * WORLD_SIZE));

        let clearance = |point: &Vec2| -> f32 {
            let asteroids = self
                .asteroids
                .iter()
                .map(|a| wrap_delta(a.position - *point).length() - a.radius());
            let ships = self
                .players
                .iter()
                .filter_map(|p| p.ship)
                .map(|s| wrap_delta(s.position - *point).length());
            asteroids.chain(ships).fold(f32::MAX, f32::min)
        };
        candidates
            .iter()
            .find(|p| clearance(p) >= SPAWN_CLEARANCE)
            .or_else(|| candidates.iter().max_by(|a, b| clearance(a).total_cmp(&clearance(b))))
            .copied()
            .unwrap_or(Vec2::ZERO)
    }

    fn new_ship(&mut self, position: Vec2, shield: f32) -> Ship {
        Ship {
            id: self.next_id(),
            position,
            velocity: Vec2::ZERO,
            angle: (-position).to_angle(),
            cooldown: 0.0,
            shield,
            thrusting: false,
        }
    }

    fn spawn_asteroid(&mut self, size: u8, position: Vec2, velocity: Vec2) {
        let id = self.next_id();
        let angle = self.rng.random_range(0.0..TAU);
        let spin = self.rng.random_range(-1.5..1.5);
        self.asteroids.push(Asteroid {
            id,
            size,
            position,
            velocity,
            angle,
            spin,
        });
    }

    fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    fn random_point(&mut self) -> Vec2 {
        Vec2::new(self.rng.random_range(-0.5..0.5), self.rng.random_range(-0.5..0.5)) * WORLD_SIZE
    }

    fn random_direction(&mut self) -> Vec2 {
        Vec2::from_angle(self.rng.random_range(0.0..TAU))
    }

    fn random_speed(&mut self, size: u8) -> f32 {
        let (min, max) = ASTEROID_SPEED[size as usize];
        self.rng.random_range(min..max)
    }
}
