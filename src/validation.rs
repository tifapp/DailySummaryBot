use std::{collections::HashMap, env};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::tracing::info;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use anyhow::{Result, anyhow};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
pub struct HttpRequest {
    pub httpMethod: Option<String>,
    pub body: Option<Vec<u8>>,
    pub headers: Option<HashMap<String, String>>,
}

pub fn verify_slack_request(req: &HttpRequest) -> Result<()> {
    let signing_secret = std::env::var("SLACK_APP_SIGNING_SECRET")
        .expect("SLACK_APP_SIGNING_SECRET environment variable should exist");

    let headers = req.headers.as_ref().ok_or_else(|| anyhow!("No headers provided"))?;

    let timestamp = headers.get("X-Slack-Request-Timestamp")
        .ok_or_else(|| anyhow!("Timestamp header missing"))?;

    let slack_signature = headers.get("X-Slack-Signature")
        .ok_or_else(|| anyhow!("Signature header missing"))?;

    let body_bytes = req.body.as_ref().ok_or_else(|| anyhow!("No body provided"))?;
    let body_str = std::str::from_utf8(body_bytes).map_err(|_| anyhow!("Body encoding error"))?;

    let basestring = format!("v0:{}:{}", timestamp, body_str);

    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .map_err(|_| anyhow!("Invalid key length for HMAC"))?;

    mac.update(basestring.as_bytes());
    let result = mac.finalize();
    let code_bytes = result.into_bytes();
    let computed_signature = format!("v0={}", hex::encode(code_bytes));

    if slack_signature != &computed_signature {
        Err(anyhow!("Verification failed. Signatures do not match."))
    } else {
        Ok(())
    }
}
