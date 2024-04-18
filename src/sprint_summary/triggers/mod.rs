mod slack_input;

use std::collections::HashMap;

use lambda_runtime::LambdaEvent;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use crate::{tracing::{error, info}, utils::http::HttpRequest};
use anyhow::{anyhow, Error, Result};

use self::slack_input::{SlackBlockActionPayload, SlackSlashCommandBody};

#[derive(Debug, Deserialize)]
pub struct Trigger {
    pub channel_id: String,
    pub command: String,
    pub text: String,
}

impl From<Triggers> for Trigger {
    fn from(data: Triggers) -> Self {
        match data {
            Triggers::SlashCommand(body) => Trigger {
                channel_id: body.channel_id,
                command: body.command,
                text: body.text,
            },
            Triggers::BlockAction(body) => Trigger {
                channel_id: body.channel.id,
                command: body.actions[0].action_id.clone(),
                text: body.actions[0].value.clone(),
            },
        }
    }
}

enum Triggers {
    SlashCommand(SlackSlashCommandBody),
    BlockAction(SlackBlockActionPayload),
}

impl From<SlackSlashCommandBody> for Triggers {
    fn from(item: SlackSlashCommandBody) -> Self {
        Triggers::SlashCommand(item)
    }
}

impl From<SlackBlockActionPayload> for Triggers {
    fn from(item: SlackBlockActionPayload) -> Self {
        Triggers::BlockAction(item)
    }
}

pub trait ConvertToTrigger {
    fn convert_to_trigger(self) -> Result<Trigger, Error>;
}

impl ConvertToTrigger for LambdaEvent<Value> {
    fn convert_to_trigger(self) -> Result<Trigger, Error> {
        let request: HttpRequest = self.into();

        info!("Converted request is {:?}", request);

        match request.http_method {
            Some(_) => {
                //make a From impl from request to slackslashcommandbody
                let slash_command_result = request.parse_request_body::<SlackSlashCommandBody>()
                    .map(Triggers::from)
                    .map(Trigger::from)
                    .ok();
            
                //make a From impl from request to SlackBlockActionPayload
                // let block_action_result = serde_json::from_str::<SlackBlockActionPayload>(&request.parse_request_body::<String>()?)
                //     .map_err(|e| {
                //         eprintln!("Failed to parse JSON body: {}", e);
                //         anyhow!("Failed to parse JSON body: {}", e)
                //     })
                //     .map(Triggers::from)
                //     .map(Trigger::from)
                //     .ok();

                let decoded_body = serde_urlencoded::from_str::<HashMap<String, String>>(&request.body.expect("should have body"))
                .map_err(|e| {
                    eprintln!("Failed to decode url-encoded body: {}", e);
                    anyhow!("Failed to decode url-encoded body: {}", e)
                })?;
            
                let block_action_result = if let Some(json_str) = decoded_body.get("payload") {
                    serde_json::from_str::<SlackBlockActionPayload>(&json_str)
                        .map_err(|e| {
                            eprintln!("Failed to parse JSON body: {}", e);
                            anyhow!("Failed to parse JSON body: {}", e)
                        })
                } else {
                    Err(anyhow!("No 'payload' key found in decoded body"))
                }.map(Triggers::from)
                .map(Trigger::from)
                .ok();

                // let block_action_result = serde_json::from_str::<SlackBlockActionPayload>(serde_urlencoded::from_str(&request.body.expect("should have body")).expect("should have a valid string body"))
                //     .map_err(|e| {
                //         eprintln!("Failed to parse JSON body: {}", e);
                //         anyhow!("Failed to parse JSON body: {}", e)
                //     })
                //     .map(Triggers::from)
                //     .map(Trigger::from)
                //     .ok();

                slash_command_result.or(block_action_result).ok_or_else(|| anyhow!("Failed to parse Slack command"))
            },
            None => {
                Ok(Trigger {
                    channel_id: "".to_string(),
                    command: "/daily-trigger".to_string(),
                    text: "No HTTP method provided".to_string(),
                })
            }
        }
    }
}
