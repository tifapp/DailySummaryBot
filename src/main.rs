mod components;
mod date;
mod summary;
mod trello;
mod github;
mod slack;
mod s3;

use std::collections::HashMap;

use lambda_http::{run, service_fn, tracing::{self, error}, Body, Error, Request, RequestExt, Response};
use anyhow::{Result};
use crate::summary::{fetch_ticket_summary_data, format_summary_message, SprintSummary};
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

async fn process_text_body(text: &str) -> Result<(), Error> {
    let params: HashMap<String, String> = serde_urlencoded::from_str(text)
        .map_err(|e| {
            error!("Failed to parse url-encoded body: {}", e);
            Error::from(e)
        })?;
    
    info!("Body (hashmap): {:?}", params);

    Ok(())
}

async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    info!("Handling request: Method: {:?}, Event: {:?}", event.method(), event);
     // Access and log the body content
     match event.body() {
        Body::Text(text) => {
            info!("Body (Text): {}", text);
            process_text_body(text).await?;
        },
        Body::Binary(binary) => {
            if let Ok(text) = std::str::from_utf8(binary) {
                info!("Body (Binary as Text): {}", text);
                process_text_body(text).await?;
            } else {
                info!("Body contains non-UTF-8 binary data");
            }
        },
        Body::Empty => {
            info!("Body is empty");
        },
    }

    //send "no sprint active" if commands used without a sprint
    
    let tickets = fetch_ticket_summary_data("KDRh6yBu").await.expect(&format!("Trello board {} should be accessible", "KDRh6yBu"));

    //if request has sprint name and end date parameters, use those. otherwise use s3 data.
    // let sprint_data = get_sprint_data(&s3_client).await?;
    //remember to overwrite the json

    // let blocks = format_summary_message(SprintSummary {
    //     name: "Test Sprint".to_string(),
    //     end_date: "09/29/2020".to_string()
    // }).await;
    let message = json!({
        "channel": "C06RRR7NBAB",
        "text": "test"
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
