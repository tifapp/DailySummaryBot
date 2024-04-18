mod slack_input;

use lambda_runtime::LambdaEvent;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use crate::{tracing::{error, info}, utils::http::HttpRequest};
use anyhow::{anyhow, Error, Result};

use self::slack_input::{SlackBlockActionBody, SlackSlashCommandBody};

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
                channel_id: body.payload.channel.id,
                command: body.payload.actions[0].action_id.clone(),
                text: body.payload.actions[0].value.clone(),
            },
        }
    }
}

enum Triggers {
    SlashCommand(SlackSlashCommandBody),
    BlockAction(SlackBlockActionBody),
}

impl From<SlackSlashCommandBody> for Triggers {
    fn from(item: SlackSlashCommandBody) -> Self {
        Triggers::SlashCommand(item)
    }
}

impl From<SlackBlockActionBody> for Triggers {
    fn from(item: SlackBlockActionBody) -> Self {
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
                let slash_command_result = request.parse_request_body::<SlackSlashCommandBody>()
                    .map(Triggers::from)
                    .map(Trigger::from)
                    .ok();

                let block_action_result = request.parse_request_body::<SlackBlockActionBody>()
                    .map(Triggers::from)
                    .map(Trigger::from)
                    .ok();

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
