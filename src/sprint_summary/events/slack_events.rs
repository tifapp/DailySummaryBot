use std::collections::HashMap;
use chrono::NaiveDate;
use serde::Deserialize;
use crate::{utils::{http::HttpRequest}};
use anyhow::{anyhow, Context, Result};
use super::SprintEvents;

#[derive(Debug, Deserialize)]
struct SlackSlashCommandBody {
    token: String,
    channel_id: String,
    user_id: String,
    command: String,
    text: String,
    api_app_id: String,
    response_url: String,
    trigger_id: String,
}

impl TryFrom<&HttpRequest> for SlackSlashCommandBody {
    type Error = anyhow::Error;

    fn try_from(request: &HttpRequest) -> Result<Self, Self::Error> {
        serde_urlencoded::from_str(&request.body)
            .map_err(|e| {
                anyhow!("Error decoding URL-encoded body: {}", e)
            })
    }
}

impl From<SlackSlashCommandBody> for SprintEvents {
    fn from(item: SlackSlashCommandBody) -> Self {
        let args = item.text.split_whitespace().map(String::from).collect::<Vec<String>>();
        let response_url = Some(item.response_url);

        match item.command.as_str() {
            "/sprint-kickoff" | "/sprint-check-in" | "/sprint-end" | "/sprint-cancel" => {
                SprintEvents::MessageTrigger {
                    command: item.command,
                    args,
                    response_url,
                    channel_id: item.channel_id
                }
            },
            _ => unimplemented!("This command is not supported yet")
        }
    }
}


#[derive(Debug, Deserialize)]
struct SlackBlockAction {
    action_id: String,
    block_id: String,
    value: String,
    #[serde(rename = "type")]
    action_type: String,
}

#[derive(Debug, Deserialize)]
struct SlackBlockActionChannel {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct SlackBlockActionPayload {
    #[serde(rename = "type")]
    trigger_type: String,
    api_app_id: String,
    token: String,
    trigger_id: String,
    actions: Vec<SlackBlockAction>,
    channel: SlackBlockActionChannel
}

impl TryFrom<&HttpRequest> for SlackBlockActionPayload {
    type Error = anyhow::Error;

    fn try_from(request: &HttpRequest) -> Result<Self, Self::Error> {
        let decoded_map: HashMap<String, String> = serde_urlencoded::from_str(&request.body)
            .map_err(|e| {
                anyhow!("Failed to decode url-encoded body: {}", e)
            })?;

        if let Some(json_str) = decoded_map.get("payload") {
            serde_json::from_str::<SlackBlockActionPayload>(json_str)
                .map_err(|e| {
                    anyhow!("Failed to parse JSON body: {}", e)
                })
        } else {
            Err(anyhow!("No 'payload' key found in decoded body"))
        }
    }
}

impl From<SlackBlockActionPayload> for SprintEvents {
    fn from(item: SlackBlockActionPayload) -> Self {
        let args: Vec<String> = item.actions[0].value.split_whitespace().map(String::from).collect::<Vec<String>>();

        match item.actions[0].action_id.as_str() {
            "/sprint-kickoff-confirm" => SprintEvents::MessageTrigger{command: item.actions[0].action_id.clone(), args, response_url: None, channel_id: item.channel.id},
            _ => unimplemented!("This command is not supported yet"),
        }
    }
}

impl TryFrom<&HttpRequest> for SprintEvents {
    type Error = anyhow::Error;

    fn try_from(request: &HttpRequest) -> Result<Self, Self::Error> {
        SlackSlashCommandBody::try_from(request)
            .map(Into::into) 
            .or_else(|_| {
                SlackBlockActionPayload::try_from(request)
                    .map(Into::into) 
            })
            .or_else(|_| {
                Err(anyhow!("Failed to parse HttpRequest into any known Slack payload type"))
            })
    }
}