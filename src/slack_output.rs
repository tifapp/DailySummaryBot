use std::env;
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::tracing::info;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use lambda_http::{Request, Body};
use anyhow::{Result, anyhow};

#[derive(Deserialize)]
struct SlackResponse {
    ok: bool,
    error: Option<String>,
}

pub async fn send_message_to_slack<T: Serialize>(channel_id: &str, blocks: &T) -> Result<()> {
    let slack_token = env::var("SLACK_OAUTH").expect("SLACK_OAUTH environment variable should exist");

    let client = reqwest::Client::new();
    
    let message = json!({
        "channel": channel_id,
        "blocks": blocks
    });

    info!("Message to Slack: {}", message);

    let response = client.post("https://slack.com/api/chat.postMessage")
        .bearer_auth(slack_token)
        .json(&message)
        .send()
        .await?;

    if response.status().is_success() {
        let response_body = response.text().await?;
        let slack_response: SlackResponse = serde_json::from_str(&response_body)
            .map_err(|e| anyhow!("Failed to deserialize Slack response: {}", e))?;

        info!("Response from Slack: {}", response_body);
        if slack_response.ok {
            Ok(())
        } else {
            Err(anyhow!("Slack API error: {}", slack_response.error.unwrap_or_else(|| "Unknown error".to_string())))
        }
    } else {
        Err(anyhow!("Failed to send message to Slack with status: {}", response.status()))
    }
}