use std::collections::HashMap;
use chrono::NaiveDate;
use serde::Deserialize;
use crate::{sprint_summary::SprintContext, utils::{date::print_current_date, http::HttpRequest}};
use anyhow::{anyhow, Context, Result};
use super::{SprintEvent, SprintEvents};

#[derive(Debug, Deserialize)]
struct SlackSlashCommandBody {
    token: String,
    channel_id: String,
    user_id: String,
    command: String,
    text: String,
    api_app_id: String
}

impl TryFrom<&HttpRequest> for SlackSlashCommandBody {
    type Error = anyhow::Error;

    fn try_from(request: &HttpRequest) -> Result<Self, Self::Error> {
        match &request.body {
            Some(body_str) => {
                serde_urlencoded::from_str(&body_str)
                    .map_err(|e| {
                        anyhow!("Error decoding URL-encoded body: {}", e)
                    })
            },
            None => Err(anyhow!("No body to parse for SlackSlashCommandBody"))
        }
    }
}

impl From<SlackSlashCommandBody> for SprintEvent {
    fn from(item: SlackSlashCommandBody) -> Self {
        let (end_date, name) = parse_sprint_params(&item.text).expect("could not convert slack message to sprint event");

        SprintEvent {
            sprint_command: item.command,
            sprint_context: SprintContext {
                start_date: print_current_date(),
                channel_id: item.channel_id,
                end_date,
                name,
            },
        }
    }
}

impl From<SlackSlashCommandBody> for SprintEvents {
    fn from(item: SlackSlashCommandBody) -> Self {
        match item.command.as_str() {
            "/sprint-kickoff" => SprintEvents::SprintPreview(item.into()),
            "/sprint-check-in" => SprintEvents::SprintCheckIn,
            _ => unimplemented!("This command is not supported yet"),
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
        match &request.body {
            Some(_) => {
                let decoded_body = request.body.as_ref().ok_or_else(|| anyhow!("No body to parse for SlackBlockActionPayload"))?;
                let decoded_map: HashMap<String, String> = serde_urlencoded::from_str(decoded_body)
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
            },
            None => Err(anyhow!("No body to parse for SlackSlashCommandBody"))
        }
    }
}

impl From<SlackBlockActionPayload> for SprintEvent {
    fn from(item: SlackBlockActionPayload) -> Self {
        let (end_date, name) = parse_sprint_params(&item.actions[0].value).expect("should be able to parse slack block action");

        SprintEvent {
            sprint_command: item.actions[0].action_id.clone(),
            sprint_context: SprintContext {
                start_date: print_current_date(),
                channel_id: item.channel.id,
                end_date,
                name,
            },
        }
    }
}

impl From<SlackBlockActionPayload> for SprintEvents {
    fn from(item: SlackBlockActionPayload) -> Self {
        match item.actions[0].action_id.as_str() {
            "/sprint-kickoff-confirm" => SprintEvents::SprintKickoff(item.into()),
            _ => unimplemented!("This command is not supported yet"),
        }
    }
}

fn parse_sprint_params(text: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return Err(anyhow!("Text field does not contain enough parts"));
    }

    let end_date = parts[0];
    let name = parts[1].to_string();

    NaiveDate::parse_from_str(end_date, "%m/%d/%Y")
        .with_context(|| format!("Failed to parse date: '{}'", end_date))?;

    Ok((end_date.to_string(), name))
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