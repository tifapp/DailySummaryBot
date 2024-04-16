use aws_sdk_s3::{Client, primitives::ByteStream};
use lambda_http::tracing::error;
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_value};
use anyhow::{Result, Context, anyhow};
use std::collections::HashMap;

use crate::{sprint_summary::SprintInput, ticket_summary::{Ticket, TicketSummary}, trello::TicketDetails};

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

impl From<&SprintRecord> for SprintInput {
    fn from(record: &SprintRecord) -> Self {
        SprintInput {
            end_date: record.end_date.clone(),
            name: record.name.clone(),
        }
    }
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

impl From<&Ticket> for TicketRecord {
    fn from(ticket: &Ticket) -> Self {
        TicketRecord {
            id: ticket.details.id.clone(),
            name: ticket.details.name.clone(),
            url: ticket.details.url.clone(),
            list_name: ticket.details.list_name.clone(),
            is_goal: ticket.details.is_goal,
            added_on: ticket.added_on.clone(),
            last_moved_on: ticket.last_moved_on.clone(),
        }
    }
}

impl From<&TicketRecord> for Ticket {
    fn from(record: &TicketRecord) -> Self {
        Ticket {
            members: vec![],
            pr: None,
            added_on: record.added_on.clone(),
            last_moved_on: record.last_moved_on.clone(),
            details: TicketDetails {            
                id: record.id.clone(),
                name: record.name.clone(),
                list_name: "None".to_string(),      
                url: record.url.clone(),                          
                has_description: true,   
                has_labels: true,                      
                is_goal: false,  
                checklist_items: 0,
                checked_checklist_items: 0,    
                is_backlogged: true,
                member_ids: vec![],
                pr_url: None,        
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketRecords {
    pub tickets: Vec<TicketRecord>
}

impl From<TicketSummary> for TicketRecords {
    fn from(summary: TicketSummary) -> Self {
        let mut tickets = Vec::new();

        let mut extend_tickets = |vec: Vec<Ticket>| {
            tickets.extend(vec.iter().map(TicketRecord::from));
        };

        extend_tickets(summary.blocked_prs);
        extend_tickets(summary.open_prs);
        extend_tickets(summary.open_tickets);
        extend_tickets(summary.completed_tickets);
        extend_tickets(summary.goal_tickets);
        extend_tickets(summary.backlogged_tickets);

        TicketRecords { tickets }
    }
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