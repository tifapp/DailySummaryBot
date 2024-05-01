use crate::utils::http::HttpRequest;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use anyhow::{Result, anyhow};

type HmacSha256 = Hmac<Sha256>;

impl HttpRequest {
    pub fn verify_slack_request(&self) -> Result<()> {
        let signing_secret = std::env::var("SLACK_APP_SIGNING_SECRET")
            .expect("SLACK_APP_SIGNING_SECRET environment variable should exist");

        let headers = self.headers.as_ref().ok_or_else(|| anyhow!("No headers provided"))?;

        let timestamp = headers.get("X-Slack-Request-Timestamp")
            .ok_or_else(|| anyhow!("Timestamp header missing"))?;

        let slack_signature = headers.get("X-Slack-Signature")
            .ok_or_else(|| anyhow!("Signature header missing"))?;

        let basestring = format!("v0:{}:{}", timestamp, self.body.as_ref().expect("should be parseable"));

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
}