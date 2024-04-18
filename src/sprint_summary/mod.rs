mod ticket;
mod ticket_summary;
mod ticket_sources;
mod triggers;
mod validation;

use std::env;
use chrono::NaiveDate;
use anyhow::{Context, Result, anyhow};
use lambda_runtime::LambdaEvent;
use serde_json::Value;
use ticket_sources::fetch_ticket_summary_data;
use crate::slack_output::send_message_to_slack;
use crate::utils::date::{days_between, print_current_date};
use crate::utils::eventbridge::{create_eventbridge_client, EventBridgeExtensions};
use crate::utils::slack_components::{context_block, header_block, primary_button_block, section_block};
use crate::utils::s3::{clear_sprint_data, clear_ticket_data, create_s3_client, get_historical_data, get_sprint_data, get_sprint_members, get_ticket_data, put_historical_data, put_sprint_data, put_ticket_data, HistoricalRecord, HistoricalRecords, SprintRecord};
use self::ticket_summary::TicketSummary;
use self::triggers::{ConvertToTrigger, Trigger};

pub struct SprintParams {
    pub end_date: String,
    pub name: String,
}

impl From<&SprintRecord> for SprintParams {
    fn from(record: &SprintRecord) -> Self {
        SprintParams {
            end_date: record.end_date.clone(),
            name: record.name.clone(),
        }
    }
}

impl SprintRecord {
    pub fn days_until_end(&self) -> u32 {
        days_between(None, &self.end_date).expect("Days until end should be parseable") as u32
    }

    pub fn total_days(&self) -> u32 {
        days_between(Some(&self.start_date), &self.end_date).expect("Total days should be parseable") as u32
    }
    
    pub fn time_indicator(&self) -> &str {
        let days_left = self.days_until_end() as f32;
        let total_days = self.total_days() as f32;
        if total_days == 0.0 {
            return "🌕";
        }
        
        let ratio = days_left / total_days;
        let emoji_index = (1.0 - ratio) * 4.0;
        
        match emoji_index.round() as i32 {
            0 => "🌕",
            1 => "🌔",
            2 => "🌓",
            3 => "🌒",
            4 | _ => "🌑",
        }
    }
}

fn parse_sprint_params(text: &str) -> Result<SprintParams> {
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return Err(anyhow!("Text field does not contain enough parts"));
    }

    let end_date = parts[0];
    let name = parts[1].to_string();

    NaiveDate::parse_from_str(end_date, "%m/%d/%Y")
        .with_context(|| format!("Failed to parse date: '{}'", end_date))?;

    Ok(SprintParams {end_date: end_date.to_string(), name})
}

pub async fn create_sprint_message(event: LambdaEvent<Value>) -> Result<()> { 
    let trigger: Trigger = event.convert_to_trigger()?;

    let s3_client = create_s3_client().await;
    let active_sprint_record = get_sprint_data(&s3_client).await?;
    let previous_ticket_data = get_ticket_data(&s3_client).await?;
    let user_mapping = get_sprint_members(&s3_client).await?; 
    let ticket_summary: TicketSummary = fetch_ticket_summary_data(previous_ticket_data, user_mapping).await?.into();
    
    let trello_board_id = env::var("TRELLO_BOARD_ID").expect("TRELLO_BOARD_ID environment variable should exist");
    let board_link_block = context_block(&format!("<https://trello.com/b/{}|View sprint board>", trello_board_id));
    //TODO: Make a function of sprint summary?

    //Maybe a constructor for sprint_summary, and then sprint_summary can have different impl methods for the different messages it can send based off a given command.
    
    match trigger.command.as_str() {
        "/sprint-kickoff-confirm" => {
            if active_sprint_record.is_some() {
                Err(anyhow!("A sprint is already in progress"))
            } else {
                let new_sprint_input = parse_sprint_params(&trigger.text)?;
                let eventbridge_client = create_eventbridge_client().await;
                eventbridge_client.create_daily_trigger_rule(&new_sprint_input.name).await?;
                let message_blocks = vec![
                    header_block(&format!("🚀 Sprint {} Kickoff: {} - {}\n\nSprint starts now!", new_sprint_input.name, print_current_date(), new_sprint_input.end_date)),
                    board_link_block
                ];
                put_ticket_data(&s3_client, &ticket_summary.into()).await?;
                put_sprint_data(&s3_client, &SprintRecord {
                    end_date: new_sprint_input.end_date,
                    name: new_sprint_input.name,
                    channel_id: trigger.channel_id.to_string(),
                    start_date: print_current_date(),
                    trello_board: env::var("TRELLO_BOARD_ID").expect("TRELLO_BOARD_ID environment variable should exist") //TODO: parameterize
                }).await?;
                send_message_to_slack(&trigger.channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        "/sprint-kickoff" => {
            if active_sprint_record.is_some() {
                Err(anyhow!("A sprint is already in progress"))
            } else {
                let new_sprint_input = parse_sprint_params(&trigger.text)?;
                let mut message_blocks = vec![
                    header_block(&format!("🔭 Sprint {} Preview: {} - {}", new_sprint_input.name, print_current_date(), new_sprint_input.end_date)),
                    section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(Some(&print_current_date()), &new_sprint_input.end_date)?))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());

                let historical_data = get_historical_data(&s3_client).await?.unwrap_or_else(|| HistoricalRecords {
                    history: Vec::new(),
                });

                if !historical_data.history.is_empty() {
                    message_blocks.extend(historical_data.into_slack_blocks());
                }

                message_blocks.push(board_link_block);
                message_blocks.push(primary_button_block("Kick Off", "/sprint-kickoff-confirm",  &trigger.text));
                send_message_to_slack(&trigger.channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        "/daily-trigger" => {            
            let record = active_sprint_record.expect("should have an active sprint saved");

            if days_between(None, &record.end_date)? > 0 {
                let mut message_blocks = vec![
                    header_block(&format!("{} Daily Summary: {}", record.time_indicator(), print_current_date())),
                    section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", ticket_summary.open_ticket_count, ticket_summary.ticket_count, record.days_until_end())),
                    section_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.push(board_link_block);

                put_ticket_data(&s3_client, &ticket_summary.into()).await?;
                
                send_message_to_slack(&trigger.channel_id, &message_blocks).await.context("Failed to send message to Slack")
            } else {
                let eventbridge_client = create_eventbridge_client().await;
                eventbridge_client.delete_daily_trigger_rule(&record.name).await?;
                clear_sprint_data(&s3_client).await?;
                clear_ticket_data(&s3_client).await?; //only clear tickets completed. add a snail to tickets that carry over.
                
                let mut message_blocks = vec![
                    header_block(&format!("🎆 Sprint {} Review: {} - {}", record.name, print_current_date(), record.end_date)),
                    header_block(&format!("\n*{}/{} Tickets* Completed in {} Days*", ticket_summary.completed_tickets.len(), ticket_summary.ticket_count, record.total_days())),
                    header_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage))
                ];

                let mut historical_data = get_historical_data(&s3_client).await?.unwrap_or_else(|| HistoricalRecords {
                    history: Vec::new(),
                });

                if !historical_data.history.is_empty() {
                    message_blocks.extend(historical_data.into_slack_blocks());
                }
                
                message_blocks.push(board_link_block);

                historical_data.history.push(HistoricalRecord {
                    name: record.name.clone(),
                    start_date: record.start_date.clone(),
                    end_date: record.end_date.clone(),
                    percent_complete: ticket_summary.completed_percentage,
                    num_tickets_complete: ticket_summary.completed_tickets.len() as u32,
                });
                
                put_historical_data(&s3_client, &historical_data).await?;
                send_message_to_slack(&trigger.channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        "/sprint-check-in" => {
            if !trigger.text.trim().is_empty() {
                Err(anyhow!("No input allowed for sprint check-in"))
            } else if active_sprint_record.is_none() {
                Err(anyhow!("No active sprint"))
            } else {
                let record = active_sprint_record.expect("should have an active sprint saved");

                let mut message_blocks = vec![
                    header_block(&format!("{} Sprint {} Check-In: {}", record.time_indicator(), record.name, print_current_date())),
                    section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", ticket_summary.open_ticket_count, ticket_summary.ticket_count, record.days_until_end())),
                    section_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage))
                ];

                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.push(board_link_block);

                put_ticket_data(&s3_client, &ticket_summary.into()).await?;
                
                send_message_to_slack(&trigger.channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        _ => Err(anyhow!("Unsupported command '{:?}'", trigger))
    }
}
//TODO: Find better way of organizing this match code