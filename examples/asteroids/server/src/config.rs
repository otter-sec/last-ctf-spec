use std::env;
use std::time::Duration;

use asteroids_protocol::MAX_PLAYERS;
use serde::Deserialize;

#[derive(Deserialize)]
struct MatchEnv {
    #[serde(rename = "matchId")]
    match_id: String,
    players: Vec<MatchEnvPlayer>,
}

#[derive(Deserialize)]
struct MatchEnvPlayer {
    #[serde(rename = "teamId")]
    team_id: String,
    name: String,
    key: String,
}

#[derive(Clone, Debug)]
pub struct MatchPlayer {
    pub team_id: String,
    pub name: String,
    pub key: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: String,
    pub match_id: String,
    pub players: Vec<MatchPlayer>,
    pub scoreboard_url: Option<String>,
    pub replay_url: Option<String>,
    pub respawn_delay: Duration,
    pub match_duration: Duration,
    pub lobby_timeout: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let raw = env::var("RCTF_MATCH").map_err(|_| "RCTF_MATCH is required".to_string())?;
        let parsed: MatchEnv = serde_json::from_str(&raw).map_err(|err| format!("RCTF_MATCH is invalid: {err}"))?;
        if parsed.players.is_empty() || parsed.players.len() > MAX_PLAYERS {
            return Err(format!("RCTF_MATCH must have 1 to {MAX_PLAYERS} players"));
        }
        if parsed.players.iter().any(|p| p.key.is_empty()) {
            return Err("RCTF_MATCH players need a key".into());
        }

        let players = parsed
            .players
            .into_iter()
            .map(|p| MatchPlayer {
                team_id: p.team_id,
                name: p.name,
                key: p.key,
            })
            .collect();

        Ok(Self {
            listen: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8443".into()),
            match_id: parsed.match_id,
            players,
            scoreboard_url: env::var("RCTF_SCOREBOARD_URL").ok().filter(|url| !url.is_empty()),
            replay_url: env::var("RCTF_REPLAY_URL").ok().filter(|url| !url.is_empty()),
            respawn_delay: duration("RESPAWN_DELAY", Duration::from_secs(5))?,
            match_duration: duration("MATCH_DURATION", Duration::from_secs(180))?,
            lobby_timeout: duration("LOBBY_TIMEOUT", Duration::from_secs(90))?,
        })
    }
}

fn duration(name: &str, default: Duration) -> Result<Duration, String> {
    let Ok(raw) = env::var(name) else {
        return Ok(default);
    };
    let invalid = || format!("{name} must look like 90s, 3m or 500ms");
    let split = raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len());
    let (number, unit) = raw.split_at(split);
    let number: u64 = number.parse().map_err(|_| invalid())?;
    let parsed = match unit {
        "ms" => Duration::from_millis(number),
        "s" | "" => Duration::from_secs(number),
        "m" => Duration::from_secs(number * 60),
        _ => return Err(invalid()),
    };
    if parsed.is_zero() {
        return Err(invalid());
    }
    Ok(parsed)
}
