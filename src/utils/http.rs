use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use lambda_runtime::{tracing::info, LambdaEvent};
use serde::Deserialize;
use serde_json::Value;
use anyhow::{Result, anyhow};

#[derive(Debug, Deserialize)]
pub struct HttpRequest {
    pub http_method: String,
    pub body: String,
    pub headers: Option<HashMap<String, String>>,
}

impl TryFrom<&LambdaEvent<Value>> for HttpRequest {
    type Error = anyhow::Error;

    fn try_from(event: &LambdaEvent<Value>) -> Result<Self, Self::Error> {
        let (value, _context) = event.clone().into_parts();
        info!("Event value is {}", value);

        let http_method = value
            .get("requestContext")
            .and_then(|rc| rc.get("http"))
            .and_then(|http| http.get("method"))
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| anyhow!("Failed to extract HTTP method from event"))?;

        let body = if value.get("isBase64Encoded").and_then(Value::as_bool) == Some(true) {
            value.get("body")
                .and_then(Value::as_str)
                .map(|b| STANDARD.decode(b).ok())
                .flatten()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .ok_or_else(|| anyhow!("Failed to decode Base64 body"))
        } else {
            value.get("body")
                .and_then(Value::as_str)
                .map(String::from)
                .ok_or_else(|| anyhow!("Failed to extract body from event"))
        }?;

        let headers = value.get("headers")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| anyhow!("Failed to parse headers"))?;

        Ok(HttpRequest {
            http_method,
            body,
            headers,
        })
    }
}
