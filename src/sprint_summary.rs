use std::env;
use chrono::NaiveDate;
use anyhow::{Context, Result, anyhow};
use crate::slack_output::send_message_to_slack;
use crate::data_sources::fetch_ticket_summary_data;
use crate::utils::date::{days_between, print_current_date};
use crate::utils::eventbridge::{create_eventbridge_client, EventBridgeExtensions};
use crate::utils::slack_components::{button_block, header_block, section_block};
use crate::utils::s3::{create_s3_client, get_sprint_data, put_sprint_data, put_ticket_data, SprintRecord};

pub struct SprintInput {
    pub end_date: String,
    pub name: String,
}

impl From<&SprintRecord> for SprintInput {
    fn from(record: &SprintRecord) -> Self {
        SprintInput {
            end_date: record.end_date.clone(),
            name: record.name.clone(),
        }
    }
}

fn parse_sprint_input(text: &str) -> Result<SprintInput> {
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return Err(anyhow!("Text field does not contain enough parts"));
    }

    let end_date = parts[0];
    let name = parts[1].to_string();

    NaiveDate::parse_from_str(end_date, "%m/%d/%Y")
        .with_context(|| format!("Failed to parse date: '{}'", end_date))?;

    Ok(SprintInput {end_date: end_date.to_string(), name})
}

pub async fn create_sprint_message(command: &str, channel_id: &str, text: &str) -> Result<()> { 
    let s3_client = create_s3_client().await;

    let active_sprint_record = get_sprint_data(&s3_client).await?;
    let tickets = fetch_ticket_summary_data().await?;

    match command {
        "/sprint-kickoff-confirm" => {
            if active_sprint_record.is_some() {
                Err(anyhow!("A sprint is already in progress"))
            } else {
                let new_sprint_input = parse_sprint_input(&text)?;
                let eventbridge_client = create_eventbridge_client().await;
                eventbridge_client.create_daily_trigger_rule(&new_sprint_input.name);
                let mut message_blocks = vec![header_block(&format!("🚀 Sprint {} Kickoff: {} - {}", new_sprint_input.name, print_current_date(), new_sprint_input.end_date))];
                message_blocks.push(section_block("Sprint starts now!"));
                message_blocks.extend(tickets.into_slack_blocks());
                put_ticket_data(&s3_client, &tickets.into()).await?;
                put_sprint_data(&s3_client, &SprintRecord {
                    end_date: new_sprint_input.end_date,
                    name: new_sprint_input.name,
                    channel_id: channel_id.to_string(),
                    start_date: print_current_date(),
                    trello_board: env::var("TRELLO_BOARD_ID").expect("TRELLO_BOARD_ID environment variable should exist") //may parameterize in the future
                }).await?;
                send_message_to_slack(&channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        //this branch should also start the eventbridge rule
        "/sprint-kickoff" => {
            if active_sprint_record.is_some() {
                Err(anyhow!("A sprint is already in progress"))
            } else {
                //add historical data viewing
                let new_sprint_input = parse_sprint_input(&text)?;
                let mut message_blocks = vec![header_block(&format!("🔭 Sprint {} Preview: {} - {}", new_sprint_input.name, print_current_date(), new_sprint_input.end_date))];
                message_blocks.push(section_block(&format!("*{} Tickets*\n*{:?} Days*", tickets.num_of_tickets, days_between(Some(&print_current_date()), &new_sprint_input.end_date)?)));
                message_blocks.extend(tickets.into_slack_blocks());
                message_blocks.push(button_block("Proceed", "/sprint-kickoff-confirm", text));
                send_message_to_slack(&channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        //add extra branch for eventbridge trigger - daily summary
        "/sprint-check-in" => {
            if !text.trim().is_empty() {
                Err(anyhow!("No input allowed for sprint check-in"))
            } else if active_sprint_record.is_none() {
                Err(anyhow!("No active sprint"))
            } else {
                let sprint_input = SprintInput::from(&active_sprint_record.expect("should have an active sprint saved"));
                let mut message_blocks = vec![header_block(&format!("🔍 Sprint {} Check-In: {}", sprint_input.name, print_current_date()))];

                let days_until_end = days_between(Some(&print_current_date()), &sprint_input.end_date)?;
                let completed_ratio = tickets.count_open_tickets() as f64 / tickets.num_of_tickets as f64;
                let completed_percentage = completed_ratio * 100.0;

                let time_warning = if days_until_end <= 4 { "⏰" } else { "" };

                message_blocks.push(section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint. {}", tickets.count_open_tickets(), tickets.num_of_tickets, days_until_end, time_warning)));
                message_blocks.push(section_block(&format!("\n*{:.2}% of tasks completed.*", completed_percentage)));
                message_blocks.extend(tickets.into_slack_blocks());

                put_ticket_data(&s3_client, &tickets.into()).await?;
                
                send_message_to_slack(&channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        //add extra branch for sprint ending
        //remember to clear the sprint data json after every sprint
        //add historical data viewing & pushing
        _ => Err(anyhow!("Unsupported command '{}'", command))
    }
}