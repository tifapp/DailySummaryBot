mod components;
mod date;
mod summary;
mod trello;
mod github;
mod slack;
mod s3;

use lambda_http::{run, service_fn, tracing, Body, Error, Request, RequestExt, Response};
use anyhow::{Result};
use crate::summary::{fetch_sprint_summary_data, format_summary_message};
use tracing::info;
use reqwest::Client;
use crate::slack::send_message_to_slack;
use serde_json::json;

//handle 4 triggers
//eventbridge
//sprint-start command
//sprint-check-in command
//sprint-extend command (record extensions)

//create a sprint kickoff message (just change header to kickoff startdate-enddate + add confirm button)
//create a sprint check-in message (change header)
//create a sprint review/summary message

//sprint complete/sprint incomplete at the end. which tells us if we met our goals or not. + %completed.

async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    info!("Handling request: Method: {:?}, Query: {:?}", event.method(), event);
    // let who = event
    //     .query_string_parameters_ref()
    //     .and_then(|params| params.first("name"))
    //     .unwrap_or("world");
    
    let board = fetch_sprint_summary_data("KDRh6yBu").await.expect(&format!("Trello board {} should be accessible", "KDRh6yBu"));
    info!("Daily Summary data: {:?}", board);

    let blocks = format_summary_message("C06RRR7NBAB", board).await;
    let message = json!({
        "channel": "C06RRR7NBAB",
        "blocks": blocks
    });
    info!("Message to Slack: {}", message);

    let client = Client::new();
    let _ = send_message_to_slack(&client, &message).await.expect("should send message to Slack without error");

    let resp = Response::builder() //should be based on webhook
        .status(200)
        .header("content-type", "text/html")
        .body("success".into())
        .map_err(Box::new)?;

    Ok(resp)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    run(service_fn(function_handler)).await
}

#[cfg(test)]
mod tests {
    use std::{env, collections::HashMap};
    use super::*;
    use lambda_http::{http::StatusCode, Body::Text};
    
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
