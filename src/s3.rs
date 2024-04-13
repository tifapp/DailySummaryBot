use aws_sdk_s3::{Client, primitives::ByteStream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_value};
use anyhow::{Result, Context, anyhow};
use std::collections::HashMap;

use crate::summary::Ticket;

async fn get_s3_json(client: &Client, key: &str) -> Result<Value> {
    let object = client
        .get_object()
        .bucket("agilesummary")
        .key(key)
        .send()
        .await
        .context("should fetch object from S3")?;
    
    let data = object
        .body
        .collect()
        .await
        .context("should read object data")?;
    
    let json = serde_json::from_slice(&data.into_bytes())
        .context("should parse JSON data")?;
    
    Ok(json)
}

async fn put_s3_json(client: &Client, key: &str, ticket_data: &Value) -> Result<()> {
    let json_data = serde_json::to_string(ticket_data)
        .context("Failed to serialize json data")?;

    let resp = client.put_object()
        .bucket("agilesummary")
        .key(key)
        .body(ByteStream::from(json_data.into_bytes()))
        .send()
        .await;

    match resp {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!(e)),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SprintRecord {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub channel: String,
    pub trello_board: String,
}

pub async fn get_sprint_data(client: &Client) -> Result<SprintRecord> {
    let json_value = get_s3_json(client, "sprint_data.json").await?;
    let sprint_status = from_value(json_value)?;
    Ok(sprint_status)
}

pub async fn put_sprint_data(client: &Client, sprint_data: &SprintRecord) -> Result<()> {
    let sprint_data_value = serde_json::to_value(sprint_data)
        .context("Failed to convert ticket data to JSON value")?;

    put_s3_json(client, "sprint_data.json", &sprint_data_value).await
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketRecord {
    pub id: String,
    pub name: String, 
    pub url: String,
    pub list_name: String,
    pub is_goal: bool,
}

impl From<Ticket> for TicketRecord {
    fn from(ticket: Ticket) -> Self {
        TicketRecord {
            id: ticket.id,
            name: ticket.name,
            url: ticket.url,
            list_name: ticket.list_name,
            is_goal: ticket.is_goal,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketRecords {
    pub tickets: Vec<TicketRecord>
}

pub async fn get_ticket_data(client: &Client) -> Result<TicketRecords> {
    let json_value = get_s3_json(client, "ticket_data.json").await?;
    let sprint_status = from_value(json_value)?;
    Ok(sprint_status)
}

pub async fn put_ticket_data(client: &Client, ticket_data: &TicketRecords) -> Result<()> {
    let ticket_data_value = serde_json::to_value(ticket_data)
        .context("Failed to convert ticket data to JSON value")?;

    put_s3_json(client, "ticket_data.json", &ticket_data_value).await
}

pub async fn get_sprint_members(client: &Client) -> Result<HashMap<String, String>> {
    let json_value = get_s3_json(client, "trello_to_slack_users.json").await?;
    let trello_to_slack_users = from_value(json_value)?;
    Ok(trello_to_slack_users)
}