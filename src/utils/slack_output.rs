use std::env;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::tracing::info;
use anyhow::{Result, anyhow};

#[derive(Deserialize)]
struct SlackResponse {
    ok: bool,
    error: Option<String>,
}

pub trait TeamCommunicationClient {
    async fn send_teams_message<T: Serialize>(&self, channel_id: &str, blocks: &T, response_url: Option<String>) -> Result<()>;
}

impl TeamCommunicationClient for Client {
    async fn send_teams_message<T: Serialize>(&self, channel_id: &str, blocks: &T, response_url: Option<String>) -> Result<()> {
        let slack_token = env::var("SLACK_OAUTH").expect("SLACK_OAUTH environment variable should exist");
        
        let message = json!({
            "channel": channel_id,
            "blocks": blocks
        });
    
        info!("Message to Slack: {}", message);
    
        let response = self.post(response_url.unwrap_or("https://slack.com/api/chat.postMessage".to_string()))
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
}