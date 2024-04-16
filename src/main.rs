mod components;
mod date;
mod ticket_summary;
mod sprint_summary;
mod trello;
mod github;
mod slack;
mod s3;

use lambda_http::{run, service_fn, tracing::{self}, Body, Request, Response};
use anyhow::{Result, anyhow, Context};
use reqwest::StatusCode;
use crate::slack::{verify_slack_request, SlackRequestBody};
use crate::sprint_summary::{create_sprint_message};
use tracing::{error, info};
use serde::de::DeserializeOwned;

//handle triggers
//eventbridge
//start sprint interactive button from kickoff message (confetti?)

//create a sprint review/summary message. sprint complete/sprint incomplete at the end. which tells us if we met our goals or not. + %completed.

fn parse_request_body<T: DeserializeOwned>(text: &Body) -> Result<T> {
    match text {
       Body::Text(text) => {
           info!("Body (Text): {}", text);
           Err(anyhow!("does not accept plain text body"))
       },
       Body::Binary(binary) => {
           if let Ok(text) = std::str::from_utf8(&binary) {
               info!("Body (Binary as Text): {}", text);
               let params: T = serde_urlencoded::from_str(text)
                   .map_err(|e| {
                       error!("Failed to parse url-encoded body: {}", e);
                       anyhow!(e)
                   })?;
           
               Ok(params)
           } else {
               Err(anyhow!("Body contains non-UTF-8 binary data"))
           }
       },
       Body::Empty => {
           Err(anyhow!("Body is empty"))
       },
   }
}


async fn function_handler(event: Request) -> Result<Response<Body>, lambda_http::Error> {
    info!("Handling request: Method: {:?}, Event: {:?}", event.method(), event);
    
    if let Err(e) = verify_slack_request(&event) {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from(format!("Invalid request: {}", e)))
            .expect("Failed to render response"));
    }

    let params: SlackRequestBody = parse_request_body(event.body())?;
    
    info!("Parsed params are: {:?}", params);

    match create_sprint_message(&params.command, &params.channel_id, &params.text).await {
        Ok(()) => {
            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html")
                .body("success".into())
                .expect("Failed to render response");
            Ok(resp)
        },
        Err(e) => {
            error!("Sending error: {:?}", e);

            Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(format!("Error processing command: {}", e)))
                .expect("Failed to render response"))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), lambda_http::Error> {
    tracing::init_default_subscriber();

    run(service_fn(function_handler)).await
}

#[cfg(test)]
mod tests {
    use std::{env, collections::HashMap};
    use super::*;
    use lambda_http::{http::StatusCode, Body::Text, RequestExt};
    use reqwest::Method;
    
    #[tokio::test]
    async fn test_function_handler() {
        env::set_var("RUST_LOG", "debug");
        let _ = env_logger::try_init();
        dotenv::dotenv().ok();
        let _ = env_logger::builder().is_test(true).try_init();

        // Simulate incoming HTTP POST request data
        let headers = [
            ("content-length", "447"),
            ("x-amzn-tls-version", "TLSv1.3"),
            ("x-forwarded-proto", "https"),
            ("x-forwarded-port", "443"),
            ("x-forwarded-for", "54.209.26.217"),
            ("accept", "application/json,*/*"),
            ("x-amzn-tls-cipher-suite", "TLS_AES_128_GCM_SHA256"),
            ("x-amzn-trace-id", "Root=1-661e3b2e-1c1e0d877d8dcc312dd57823;Parent=7810b7bb797c88b8;Sampled=0;Lineage=f8c01797:0"),
            ("host", "6zyr7rber5tcwxhf5ji5f266hi0sgvuh.lambda-url.us-west-2.on.aws"),
            ("content-type", "application/x-www-form-urlencoded"),
            ("x-slack-request-timestamp", "1713257262"),
            ("x-slack-signature", "v0=caef35c6b7296dcf7fbe4b1ea0fba1cdf7b24653f27bb3ef719b2c64e76e6bad"),
            ("accept-encoding", "gzip,deflate"),
            ("user-agent", "Slackbot 1.0 (+https://api.slack.com/robots)"),
        ];

        let body = serde_urlencoded::to_string([
            ("token", "JxPzi5d87tmKzBvRUAyE9bIX"),
            ("team_id", "T01BFE465AN"),
            ("team_domain", "tif-corp"),
            ("channel_id", "C06RRR7NBAB"),
            ("channel_name", "daily-summary"),
            ("user_id", "U01CL8PLU72"),
            ("user_name", "seanim0920"),
            ("command", "/sprint-kickoff"),
            ("text", "09/20/2025 test"),
            ("api_app_id", "A0527UHK2F3"),
            ("is_enterprise_install", "false"),
            ("response_url", "https://hooks.slack.com/commands/T01BFE465AN/6981624816737/DjjVeStvAKmQlbIXoPvXymce"),
            ("trigger_id", "696604658574949.1389480209362.6f38e7650555ab0cce5dc915f2c4fb1a0")
        ]).expect("Failed to serialize test body");

        let mut request = Request::new(body.into());
        *request.method_mut() = Method::POST;
        for (header_name, header_value) in headers.iter() {
            request.headers_mut().insert(*header_name, header_value.parse().unwrap());
        }

        let expected_response = "success";

        match function_handler(request).await {
            Ok(response) => {
                assert_eq!(response.status(), StatusCode::OK);
                match response.body() {
                    Text(body_str) => {
                        assert_eq!(body_str, expected_response, "The response body does not match the expected response");
                    },
                    _ => panic!("Response body is not text"),
                }
            },
            Err(e) => panic!("Handler returned an error: {:?}", e),
        }
    }
}
