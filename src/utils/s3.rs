use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::{Client, primitives::ByteStream};
use lambda_runtime::tracing::error;
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_value};
use anyhow::{Result, Context, anyhow};
use std::collections::{HashMap, VecDeque};

use super::slack_components::section_block;

async fn get_s3_json(client: &Client, key: &str) -> Result<Option<Value>> {
    let object = match client.get_object()
        .bucket("agilesummary")
        .key(key)
        .send()
        .await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to fetch object from S3: {}", e);
            return Ok(None);  // Log the error and return None
        }
    };

    let data = match object.body.collect().await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read object data: {}", e);
            return Ok(None);  // Log the error and return None
        }
    };

    match serde_json::from_slice::<Value>(&data.into_bytes()) {
        Ok(json) => Ok(Some(json)),  // Successfully parsed JSON
        Err(e) => {
            error!("Failed to parse JSON data: {}", e);
            Ok(None)  // Log the error and return None
        }
    }
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
        Err(e) => Err(anyhow!("Failed to insert json: {}", e)),
    }
}

async fn delete_s3_json(client: &Client, key: &str) -> Result<()> {
    let resp = client.delete_object()
        .bucket("agilesummary")
        .key(key)
        .send()
        .await;

    match resp {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!("Failed to delete json: {}", e)),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SprintRecord {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub channel_id: String,
    pub trello_board: String,
}

pub async fn get_sprint_data(client: &Client) -> Result<Option<SprintRecord>> {
    get_s3_json(client, "sprint_data.json").await?
        .map(|json_value| {
            from_value::<SprintRecord>(json_value)
                .context("Failed to deserialize sprint data")
        })
        .transpose()
}

pub async fn put_sprint_data(client: &Client, sprint_data: &SprintRecord) -> Result<()> {
    let sprint_data_value = serde_json::to_value(sprint_data)
        .context("Failed to convert ticket data to JSON value")?;

    put_s3_json(client, "sprint_data.json", &sprint_data_value).await
}

pub async fn clear_sprint_data(client: &Client) -> Result<()> {
    delete_s3_json(client, "sprint_data.json").await
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketRecord {
    pub id: String,
    pub name: String, 
    pub url: String,
    pub list_name: String,
    pub is_goal: bool,
    pub added_on: String,
    pub last_moved_on: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketRecords {
    pub tickets: VecDeque<TicketRecord>
}

pub async fn get_ticket_data(client: &Client) -> Result<Option<TicketRecords>> {
    get_s3_json(client, "ticket_data.json").await?
        .map(|json_value| {
            from_value::<TicketRecords>(json_value)
                .context("Failed to deserialize sprint data")
        })
        .transpose()
}

pub async fn put_ticket_data(client: &Client, ticket_data: &TicketRecords) -> Result<()> {
    let ticket_data_value = serde_json::to_value(ticket_data)
        .context("Failed to convert ticket data to JSON value")?;

    put_s3_json(client, "ticket_data.json", &ticket_data_value).await
}

pub async fn clear_ticket_data(client: &Client) -> Result<()> {
    delete_s3_json(client, "ticket_data.json").await
}

pub async fn get_sprint_members(client: &Client) -> Result<Option<HashMap<String, String>>> {
    get_s3_json(client, "trello_to_slack_users.json").await?
        .map(|json_value| {
            from_value::<HashMap<String, String>>(json_value)
                .context("Failed to deserialize sprint data")
        })
        .transpose()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoricalRecord {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub percent_complete: f64,
    pub num_tickets_complete: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoricalRecords {
    pub history: Vec<HistoricalRecord>
}


impl HistoricalRecords {
    pub fn into_slack_blocks(&self) -> Vec<Value> {
        if self.history.is_empty() {
            return vec![]
        }

        let mut blocks: Vec<serde_json::Value> = vec![section_block("\n\n*Previous Sprints:*")];

        blocks.extend(self.history.iter().map(|record| {
            section_block(&format!(
                "{} - {}: *{} tickets | {:.2}%*",
                record.start_date,
                record.end_date,
                record.num_tickets_complete,
                record.percent_complete
            ))
        }));

        blocks
    }
}

pub async fn get_historical_data(client: &Client) -> Result<Option<HistoricalRecords>> {
    get_s3_json(client, "historical_data.json").await?
        .map(|json_value| {
            from_value::<HistoricalRecords>(json_value)
                .context("Failed to deserialize sprint data")
        })
        .transpose()
}

pub async fn put_historical_data(client: &Client, historical_data: &HistoricalRecords) -> Result<()> {
    let historical_data_value = serde_json::to_value(historical_data)
        .context("Failed to convert historical data to JSON value")?;

    put_s3_json(client, "historical_data.json", &historical_data_value).await
}

pub async fn clear_historical_data(client: &Client) -> Result<()> {
    delete_s3_json(client, "historical_data.json").await
}

pub async fn create_s3_client() -> Client {
    let region_provider = RegionProviderChain::default_provider().or_else("us-west-2");
    let config = aws_config::from_env().region(region_provider).load().await;
    aws_sdk_s3::Client::new(&config)
}