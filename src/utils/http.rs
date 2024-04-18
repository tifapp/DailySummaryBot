use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use lambda_runtime::{tracing::info, LambdaEvent};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use anyhow::{Result, anyhow};

#[derive(Debug, Deserialize)]
pub struct HttpRequest {
    pub http_method: Option<String>,
    pub body: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

impl From<LambdaEvent<Value>> for HttpRequest {
    fn from(event: LambdaEvent<Value>) -> Self {
        let (value, _context) = event.into_parts();

        info!("Event value is {}", value);

        HttpRequest {
            http_method: value.get("httpMethod").and_then(Value::as_str).map(String::from),
            body: value.get("body").and_then(Value::as_str).map(String::from),
            headers: value.get("headers").and_then(|v| serde_json::from_value(v.clone()).ok()),
        }
    }
}

impl HttpRequest {
    pub fn parse_request_body<T: DeserializeOwned>(&self) -> Result<T> {
        match &self.body {
            Some(b64) => {
                let decoded_bytes = STANDARD.decode(b64)
                    .map_err(|_| anyhow!("Failed to decode Base64 data"))?;
                let decoded_str = String::from_utf8(decoded_bytes)
                    .map_err(|_| anyhow!("Decoded bytes are not valid UTF-8"))?;
                serde_urlencoded::from_str(&decoded_str)
                    .map_err(|e| {
                        eprintln!("Failed to parse url-encoded body: {}", e);
                        anyhow!("Failed to parse url-encoded body: {}", e)
                    })
            },
            None => Err(anyhow!("No body to parse"))
        }
    }
}
