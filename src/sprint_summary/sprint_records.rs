use std::{self, collections::VecDeque};
use std::collections::HashMap;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{from_value, Value};

use crate::utils::s3::JsonStorageClient;
use crate::utils::slack_components::section_block;

pub trait SprintMemberClient {
    async fn get_sprint_members(&self) -> Result<Option<HashMap<String, String>>>;
}

impl<T> SprintMemberClient for T where T: JsonStorageClient, {
    async fn get_sprint_members(&self) -> Result<Option<HashMap<String, String>>> {
        self.get_json("trello_to_slack_users.json").await?
            .map(|json_value| {
                from_value::<HashMap<String, String>>(json_value)
                    .context("Failed to deserialize sprint data")
            })
            .transpose()
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

pub trait SprintRecordClient {
    async fn get_sprint_data(&self) -> Result<Option<SprintRecord>>;
    async fn put_sprint_data(&self, sprint_record: &SprintRecord) -> Result<()>;
    async fn clear_sprint_data(&self) -> Result<()>;
}

impl<T> SprintRecordClient for T where T: JsonStorageClient, {
    async fn get_sprint_data(&self) -> Result<Option<SprintRecord>> {
        self.get_json("sprint_data.json").await?
            .map(|json_value| {
                from_value::<SprintRecord>(json_value)
                    .context("Failed to deserialize sprint data")
            })
            .transpose()
    }
    
    async fn put_sprint_data(&self, sprint_data: &SprintRecord) -> Result<()> {
        let sprint_data_value = serde_json::to_value(sprint_data)
            .context("Failed to convert ticket data to JSON value")?;
    
        self.put_json("sprint_data.json", &sprint_data_value).await
    }
    
    async fn clear_sprint_data(&self) -> Result<()> {
        self.delete_json("sprint_data.json").await
    }
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

pub trait TicketRecordClient {
    async fn get_ticket_data(&self) -> Result<Option<TicketRecords>>;
    async fn put_ticket_data(&self, ticket_data: &TicketRecords) -> Result<()>;
}

impl<T> TicketRecordClient for T where T: JsonStorageClient, {
    async fn get_ticket_data(&self) -> Result<Option<TicketRecords>> {
        self.get_json("ticket_data.json").await?
            .map(|json_value| {
                from_value::<TicketRecords>(json_value)
                    .context("Failed to deserialize sprint data")
            })
            .transpose()
    }

    async fn put_ticket_data(&self, ticket_data: &TicketRecords) -> Result<()> {
        let ticket_data_value = serde_json::to_value(ticket_data)
            .context("Failed to convert ticket data to JSON value")?;

        self.put_json("ticket_data.json", &ticket_data_value).await
    }
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

pub trait HistoricalRecordClient {
    async fn get_historical_data(&self) -> Result<Option<HistoricalRecords>>;
    async fn put_historical_data(&self, ticket_data: &HistoricalRecords) -> Result<()>;
}

impl<T> HistoricalRecordClient for T where T: JsonStorageClient, {
    async fn get_historical_data(&self) -> Result<Option<HistoricalRecords>> {
        self.get_json("historical_data.json").await?
            .map(|json_value| {
                from_value::<HistoricalRecords>(json_value)
                    .context("Failed to deserialize sprint data")
            })
            .transpose()
    }
    
    async fn put_historical_data(&self, historical_data: &HistoricalRecords) -> Result<()> {
        let historical_data_value = serde_json::to_value(historical_data)
            .context("Failed to convert historical data to JSON value")?;
    
        self.put_json("historical_data.json", &historical_data_value).await
    }
}