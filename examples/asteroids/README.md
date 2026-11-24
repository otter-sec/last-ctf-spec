# asteroids

Free-for-all Asteroids for 2 to 4 teams, played in matches of a few minutes. You score by shooting asteroids while everyone else is after the same ones. Shooting another ship scores nothing, but it takes that ship out of the game for a few seconds, and it can't take your asteroids while it's gone.

| Crate | |
| --- | --- |
| [`protocol`](protocol) | Wire format and replay format |
| [`server`](server) | Authoritative match server, plain UDP |
| [`client`](client) | Bevy client, for playing and for watching replays |

## Running a match

The server plays one match. `RCTF_MATCH` tells it who is playing:

```sh
RCTF_MATCH='{"matchId":"local","players":[{"teamId":"a","name":"A","key":"key-a"},{"teamId":"b","name":"B","key":"key-b"}]}' \
    cargo run --release -p asteroids-server

cargo run --release -p asteroids-client -- --server udp://127.0.0.1:8443 --key key-a
cargo run --release -p asteroids-client -- --server udp://127.0.0.1:8443 --key key-b
```

## Network

The server speaks plain UDP. A player is the address a `Hello` with a team's key came from, until it has been quiet for 5 seconds.

At an event the server sits behind a proxy that takes QUIC, picks the match by hostname and passes each QUIC datagram on as one UDP packet. `--server <host>` connects to that proxy, `--server udp://<host>:<port>` to a server directly.

## Server

| Variable | Default | |
| --- | --- | --- |
| `RCTF_MATCH` | required | Match id, and each team's id, name and key |
| `RCTF_SCOREBOARD_URL` | | Where the scoreboard is PUT when the match is over. Without it the result is only logged. |
| `RCTF_REPLAY_URL` | | Where the replay is PUT, before the scoreboard |
| `MATCH_DURATION` | `180s` | `90s`, `3m`, `500ms`… |
| `LOBBY_TIMEOUT` | `90s` | How long to wait for every team to connect |
| `RESPAWN_DELAY` | `5s` | How long a destroyed ship is out of the game |
| `LISTEN_ADDR` | `0.0.0.0:8443` | UDP |

- The match starts when every team has connected, or after `LOBBY_TIMEOUT` with the teams that have. A team that turns up later sits it out.
- Asteroids are worth 20, 50 or 100 points, the smaller the more.
- A ship is destroyed when it gets shot or flies into an asteroid or another ship. It comes back after `RESPAWN_DELAY`, with a shield for 3 seconds.

When the match is over the server reports a scoreboard, a score per team that adds up to the number of teams. Every team gets `2/(N-1)` for each team it finished ahead of, and half of that for each one it tied with:

```json
{"scores": {"a": 2.0, "b": 0.0}}
```

With fewer than two teams connected there was no match, and it reports `{"void": true, "reason": "…"}`.

## Docker

```sh
docker build --platform linux/amd64 -f server/Dockerfile -t asteroids-server .
```
