use std::time::Duration;

use bytes::Bytes;
use reqwest::StatusCode;
use tracing::{error, info, warn};

const ATTEMPTS: u32 = 8;

pub async fn report(replay_url: Option<String>, replay: Vec<u8>, scoreboard_url: Option<String>, scoreboard: String) {
    let client = reqwest::Client::new();
    match replay_url {
        Some(url) => put(&client, &url, "replay", "application/octet-stream", replay.into()).await,
        None => warn!("RCTF_REPLAY_URL is not set, not uploading the replay"),
    }
    match scoreboard_url {
        Some(url) => put(&client, &url, "scoreboard", "application/json", scoreboard.into()).await,
        None => warn!("RCTF_SCOREBOARD_URL is not set, the result of the match goes nowhere"),
    }
}

async fn put(client: &reqwest::Client, url: &str, what: &str, content_type: &str, body: Bytes) {
    for attempt in 1..=ATTEMPTS {
        let response = client
            .put(url)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body.clone())
            .send()
            .await;
        match response {
            Ok(response) if matches!(response.status(), StatusCode::ACCEPTED | StatusCode::CONFLICT) => {
                info!(
                    status = response.status().as_u16(),
                    bytes = body.len(),
                    what,
                    "handed to the submitter"
                );
                return;
            }
            Ok(response) if response.status().is_client_error() => {
                let status = response.status().as_u16();
                let detail = response.text().await.unwrap_or_default();
                error!(status, detail, what, "the submitter rejected it");
                return;
            }
            Ok(response) => warn!(
                status = response.status().as_u16(),
                attempt, what, "the submitter failed"
            ),
            Err(err) => warn!(%err, attempt, what, "submitter unreachable"),
        }
        tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
    }
    error!(what, "giving up on handing it to the submitter");
}
