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

        let http_method = value
            .get("requestContext")  
            .and_then(|rc| rc.get("http"))
            .and_then(|http| http.get("method"))
            .and_then(Value::as_str)
            .map(String::from);
        
        let body = if value.get("isBase64Encoded").and_then(Value::as_bool) == Some(true) {
            value.get("body")
                .and_then(Value::as_str)
                .map(|b| STANDARD.decode(b).ok())
                .flatten()
                .and_then(|bytes| String::from_utf8(bytes).ok())
        } else {
            value.get("body").and_then(Value::as_str).map(String::from)
        };

        let headers = value.get("headers")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        HttpRequest {
            http_method,
            body,
            headers,
        }
    }
}

impl HttpRequest {
    pub fn parse_request_body<T: DeserializeOwned>(&self) -> Result<T> {
        match &self.body {
            Some(body_str) => {
                serde_urlencoded::from_str(&body_str)
                    .map_err(|e| {
                        eprintln!("Failed to parse url-encoded body: {}", e);
                        anyhow!("Failed to parse url-encoded body: {}", e)
                    })
            },
            None => Err(anyhow!("No body to parse"))
        }
    }
}
