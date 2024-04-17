use std::env;
use serde::{Deserialize, Serialize};
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


#[derive(Debug, Deserialize)]
pub struct SlackSlashCommandBody {
    pub token: String,
    pub channel_id: String,
    pub user_id: String,
    pub command: String,
    pub text: String,
    pub api_app_id: String
}

#[derive(Debug, Deserialize)]
pub struct SlackBlockAction {
    pub action_id: String,
    pub block_id: String,
    pub value: String,
    #[serde(rename = "type")]
    pub action_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SlackBlockActionChannel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SlackBlockActionPayload {
    #[serde(rename = "type")]
    pub trigger_type: String,
    pub api_app_id: String,
    pub token: String,
    pub trigger_id: String,
    pub actions: Vec<SlackBlockAction>,
    pub channel: SlackBlockActionChannel
}

#[derive(Debug, Deserialize)]
pub struct SlackBlockActionBody {
    pub payload: SlackBlockActionPayload,
}