mod sprint_summary;
mod utils;

use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{error, info};
use crate::sprint_summary::{SprintEventMessageGenerator, SprintEventParser};
use crate::utils::slack_output::TeamCommunicationClient;

async fn function_handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
    info!("Input is: {:?}", event);

    let response_url = event.payload["response_url"].as_str().unwrap_or_default().to_string();
    info!("response_url is: {:?}", response_url);

    let sprint_event_result = event.try_into_sprint_event().await;
    match sprint_event_result {
        Ok(sprint_event) => {
            info!("sprint event is valid: {:?}", sprint_event);
            let response = format!(
                "Valid sprint command received: {:?}",
                sprint_event
            );

            tokio::spawn(async move {
                info!("preparing sprint message");

                let fetch_client = Client::new();
                let sprint_message = &sprint_event.create_sprint_event_message(&fetch_client).await.expect("should generate sprint message");
                match fetch_client.send_teams_message(&sprint_event.sprint_context.channel_id, sprint_message).await {
                    Ok(()) => info!("Processed command successfully"),
                    Err(e) => error!("Error processing command: {:?}", e),
                }
            });

            Ok(json!({
                "statusCode": 200,
                "headers": { "Content-Type": "text/html" },
                "body": response
            }))
        },
        Err(e) => {
            error!("Error converting lambda event to sprint event: {:?}", e);
            Ok(json!({
                "statusCode": 400,
                "headers": { "Content-Type": "text/html" },
                "body": format!("Error: Failed to convert event to sprint event: {}", e)
            }))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    //from here, pass validation to function handler?

    run(service_fn(function_handler)).await
}

// #[cfg(test)]
// mod tests {
//     use std::{collections::HashMap, env};
//     use super::*;
//     use reqwest::Method;
    
//     #[tokio::test]
//     async fn test_function_handler() {
//         env::set_var("RUST_LOG", "debug");
//         let _ = env_logger::try_init();
//         dotenv::dotenv().ok();
//         let _ = env_logger::builder().is_test(true).try_init();
        
//         let headers: HashMap<String, String> = HashMap::from([
//             ("content-length", "447"),
//             ("x-amzn-tls-version", "TLSv1.3"),
//             ("x-forwarded-proto", "https"),
//             ("x-forwarded-port", "443"),
//             ("x-forwarded-for", "54.209.26.217"),
//             ("accept", "application/json,*/*"),
//             ("x-amzn-tls-cipher-suite", "TLS_AES_128_GCM_SHA256"),
//             ("x-amzn-trace-id", "Root=1-661e3b2e-1c1e0d877d8dcc312dd57823;Parent=7810b7bb797c88b8;Sampled=0;Lineage=f8c01797:0"),
//             ("host", "6zyr7rber5tcwxhf5ji5f266hi0sgvuh.lambda-url.us-west-2.on.aws"),
//             ("content-type", "application/x-www-form-urlencoded"),
//             ("x-slack-request-timestamp", "1713257262"),
//             ("x-slack-signature", "v0=caef35c6b7296dcf7fbe4b1ea0fba1cdf7b24653f27bb3ef719b2c64e76e6bad"),
//             ("accept-encoding", "gzip,deflate"),
//             ("user-agent", "Slackbot 1.0 (+https://api.slack.com/robots)"),
//         ]);

//         //should be inside payload field
//         let body = serde_urlencoded::to_string([
//             ("token", "JxPzi5d87tmKzBvRUAyE9bIX"),
//             ("team_id", "T01BFE465AN"),
//             ("team_domain", "tif-corp"),
//             ("channel_id", "C06RRR7NBAB"),
//             ("channel_name", "daily-summary"),
//             ("user_id", "U01CL8PLU72"),
//             ("user_name", "seanim0920"),
//             ("command", "/sprint-kickoff"),
//             ("text", "09/20/2025 test"),
//             ("api_app_id", "A0527UHK2F3"),
//             ("is_enterprise_install", "false"),
//             ("response_url", "https://hooks.slack.com/commands/T01BFE465AN/6981624816737/DjjVeStvAKmQlbIXoPvXymce"),
//             ("trigger_id", "696604658574949.1389480209362.6f38e7650555ab0cce5dc915f2c4fb1a0")
//         ]).expect("Failed to serialize test body");

//         let event = HttpRequest {
//             httpMethod: Some("POST".to_string()),
//             body: Some(body.into_bytes()),
//             headers: Some(headers),
//         };

//         let expected_response = "success";

//         match function_handler(event.into()).await {
//             Ok(response) => {
//                 assert_eq!(response, expected_response, "The response body does not match the expected response");
//             },
//             Err(e) => panic!("Handler returned an error: {:?}", e),
//         }
//     }
// }
