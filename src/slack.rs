use std::env;
use serde::{Deserialize, Serialize};
use reqwest::Client;
use serde_json::json;
use crate::tracing::info;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use lambda_http::{Request, Body};
use anyhow::{Result, anyhow};

type HmacSha256 = Hmac<Sha256>;

pub fn verify_slack_request(req: &Request) -> Result<()> {
    let signing_secret = env::var("SLACK_APP_SIGNING_SECRET").expect("SLACK_APP_SIGNING_SECRET environment variable should exist");

    let timestamp = req.headers()
        .get("X-Slack-Request-Timestamp")
        .ok_or_else(|| anyhow!("Timestamp header missing"))?
        .to_str()?
        .to_owned();

    let slack_signature = req.headers()
        .get("X-Slack-Signature")
        .ok_or_else(|| anyhow!("Signature header missing"))?
        .to_str()?
        .to_owned();

    let body = req.body();
    let body_str = match body {
        Body::Text(text) => text,
        Body::Binary(bin) => std::str::from_utf8(bin)?,
        Body::Empty => "",
    };

    let basestring = format!("v0:{}:{}", timestamp, body_str);

    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .map_err(|_| anyhow!("Invalid key length for HMAC"))?;

    mac.update(basestring.as_bytes());
    let result = mac.finalize();
    let code_bytes = result.into_bytes();
    let computed_signature = format!("v0={}", hex::encode(code_bytes));

    if slack_signature != computed_signature {
        Err(anyhow!("Verification failed. Signatures do not match."))
    } else {
        Ok(())
    }
}


#[derive(Deserialize)]
pub struct SlackRequestBody {
    pub token: String, //JxPzi5d87tmKzBvRUAyE9bIX,
    pub channel_id: String, // C06RRR7NBAB,
    pub user_id: String, //U01CL8PLU72,
    pub command: String, ///sprint-kickoff,
    pub text: String, //09/20/2025 test,
    pub api_app_id: String // A0527UHK2F3,
}

#[derive(Deserialize)]
struct SlackResponse {
    ok: bool,
    error: Option<String>,
}

//may need a success and failure unit test here
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