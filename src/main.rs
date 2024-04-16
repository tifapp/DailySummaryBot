mod components;
mod date;
mod ticket_summary;
mod sprint_summary;
mod trello;
mod github;
mod slack;
mod s3;

use aws_config::meta::region::RegionProviderChain;
use lambda_http::{run, service_fn, tracing::{self, error}, Body, Request, Response};
use anyhow::{Result, anyhow, Context};
use reqwest::StatusCode;
use crate::{s3::get_sprint_data, slack::{verify_slack_request, SlackRequestBody}, sprint_summary::{create_sprint_message, generate_summary_message}, ticket_summary::{create_ticket_summary, fetch_ticket_summary_data}};
use tracing::info;
use crate::slack::send_message_to_slack;
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

    let message = create_sprint_message(&params.command, &params.channel_id, &params.text).await;

    match message {
        Ok(msg) => {
            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html")
                .body("success".into())
                .expect("Failed to render response");
            Ok(resp)
        },
        Err(e) => {
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
    
    #[tokio::test]
    async fn test_function_handler() {
        env::set_var("RUST_LOG", "debug");
        let _ = env_logger::try_init();
        dotenv::dotenv().ok();

        let request = Request::new(lambda_http::Body::Empty).with_query_string_parameters(vec![("name", "Tester")].iter().map(|&(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<String, String>>());
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
