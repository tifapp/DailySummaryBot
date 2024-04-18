use std::env;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::tracing::info;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use anyhow::{Result, anyhow};

#[derive(Debug, Deserialize)]
pub struct SlackSlashCommandBody {
    pub token: String,
    pub channel_id: String,
    pub user_id: String,
    pub command: String,
    pub text: String,
    pub api_app_id: String
}

#[derive(Debug, Deserialize)]
pub struct SlackBlockAction {
    pub action_id: String,
    pub block_id: String,
    pub value: String,
    #[serde(rename = "type")]
    pub action_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SlackBlockActionChannel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SlackBlockActionPayload {
    #[serde(rename = "type")]
    pub trigger_type: String,
    pub api_app_id: String,
    pub token: String,
    pub trigger_id: String,
    pub actions: Vec<SlackBlockAction>,
    pub channel: SlackBlockActionChannel
}

#[derive(Debug, Deserialize)]
pub struct SlackBlockActionBody {
    pub payload: SlackBlockActionPayload,
}