mod slack_input;

use serde::{de::DeserializeOwned, Deserialize};
use crate::tracing::{error, info};
use lambda_http::Body;
use anyhow::{Result, anyhow};

use self::slack_input::{SlackBlockActionBody, SlackSlashCommandBody};

#[derive(Debug, Deserialize)]
pub struct Command {
    pub channel_id: String,
    pub command: String,
    pub text: String,
}

impl From<Triggers> for Command {
    fn from(data: Triggers) -> Self {
        match data {
            Triggers::SlashCommand(body) => Command {
                channel_id: body.channel_id,
                command: body.command,
                text: body.text,
            },
            Triggers::BlockAction(body) => Command {
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

pub fn parse_command(body: &Body) -> Result<Command, anyhow::Error> {
    let trigger_result = parse_request_body::<SlackSlashCommandBody>(body)
        .map(Triggers::from)
        .or_else(|_| parse_request_body::<SlackBlockActionBody>(body).map(Triggers::from));

    match trigger_result {
        Ok(triggers) => Ok(Command::from(triggers)),
        Err(e) => Err(e),
    }
}