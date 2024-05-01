mod sprint_summary;
mod utils;

use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{error, info};
use crate::sprint_summary::{SprintCommandParser, events::MapToSprintEvents};
use crate::utils::s3::create_json_storage_client;
use crate::utils::slack_output::TeamCommunicationClient;
use crate::sprint_summary::sprint_records::{DailyTicketContextClient, ActiveSprintContextClient, CumulativeSprintContextClient, SprintMemberClient};
use crate::sprint_summary::ticket_sources::TicketSummaryClient;
use sprint_summary::sprint_records::{CumulativeSprintContexts,DailyTicketContexts};

#[cfg(not(test))]
async fn function_handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
    use std::collections::{HashMap, VecDeque};

    use sprint_summary::{events::SprintEvents, SprintCommand};
    use utils::eventbridge::create_eventbridge_client;


    info!("Input is: {:?}", event);

    let sprint_client = create_json_storage_client().await;

    let active_sprint_context = sprint_client.get_sprint_data().await?;
    let previous_ticket_data = sprint_client.get_ticket_data().await?.unwrap_or(DailyTicketContexts {
        tickets: VecDeque::new(),
    });
    let user_mapping = sprint_client.get_sprint_members().await?.unwrap_or(HashMap::new());
    let mut cumulative_sprint_contexts = sprint_client.get_historical_data().await?.unwrap_or(CumulativeSprintContexts {
        history: Vec::new(),
    });

    let sprint_events = event.try_into_sprint_events().expect("Failed to parse sprint events");
    
    let (channel_id, response_url) = match &sprint_events {
        SprintEvents::MessageTrigger { channel_id, response_url, .. } => (channel_id.clone(), response_url.clone()),
        _ => (active_sprint_context.as_ref().unwrap().channel_id.clone(), None)
    };

    let sprint_command_result = sprint_events.try_into_sprint_command(&active_sprint_context, &cumulative_sprint_contexts).await;

    match sprint_command_result {
        Ok(sprint_command) => {
            info!("Sprint event is valid: {:?}", sprint_command);
            
            let fetch_client = Client::new();
            let name = match &sprint_command {
                SprintCommand::SprintPreview { sprint_name, .. } | SprintCommand::SprintKickoff { sprint_name, .. } => sprint_name,
                _ => &active_sprint_context.as_ref().unwrap().name,
            };

            let mut ticket_summary = fetch_client.fetch_ticket_summary(name, &cumulative_sprint_contexts, &previous_ticket_data, user_mapping).await?;
            let notification_client = create_eventbridge_client().await;

            let sprint_message = sprint_command.create_sprint_message(&ticket_summary, &active_sprint_context, &cumulative_sprint_contexts, &previous_ticket_data).await.expect("should generate sprint message");
            sprint_command.save_sprint_state(&mut ticket_summary, &active_sprint_context, &mut cumulative_sprint_contexts, &sprint_client, &notification_client).await.expect("should update sprint state");

            match fetch_client.send_teams_message(&channel_id, &sprint_message, response_url).await {
                Ok(()) => Ok(json!("Processed command successfully")),
                Err(e) => Ok(json!(format!("Error processing command: {:?}", e))),
            }
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
#[cfg(not(test))]
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
