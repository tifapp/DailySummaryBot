mod ticket;
mod ticket_summary;
mod ticket_sources;
mod sprint_records;
mod events;

use std::collections::{HashMap, VecDeque};
use std::env;
use anyhow::{Result, anyhow};
use lambda_runtime::tracing::info;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use crate::utils::date::{days_between, print_current_date};
use crate::utils::eventbridge::{create_eventbridge_client, EventBridgeExtensions};
use crate::utils::s3::create_json_storage_client;
use crate::utils::slack_components::{context_block, header_block, primary_button_block, section_block};
use self::sprint_records::{
    CumulativeSprintContext, 
    CumulativeSprintContextClient, 
    CumulativeSprintContexts, 
    SprintMemberClient, 
    LiveSprintContext, 
    LiveSprintContextClient, 
    DailyTicketContextClient, 
    DailyTicketContexts
};
use self::ticket_sources::TicketSummaryClient;

#[derive(Debug, Deserialize)]
pub struct SprintContext {
    pub channel_id: String,
    pub start_date: String,
    pub end_date: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SprintEvent {
    pub sprint_command: String,
    pub response_url: Option<String>,
    pub sprint_context: SprintContext,
}

pub trait SprintEventParser {
    async fn try_into_sprint_event(self) -> Result<SprintEvent>;
}

impl SprintContext {
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

pub trait SprintEventMessageGenerator {
    async fn create_sprint_event_message(&self, fetch_client: &Client) -> Result<Vec<Value>>;
}

impl SprintEventMessageGenerator for SprintEvent {
    async fn create_sprint_event_message(&self, fetch_client: &Client) -> Result<Vec<Value>> {
        info!("Going to start making sprint message");
        let json_storage_client = create_json_storage_client().await;
        info!("Made json sprint client");
        
        let previous_ticket_data = json_storage_client.get_ticket_data().await?.unwrap_or(DailyTicketContexts {
            tickets: VecDeque::new(),
        });
        info!("Have previous ticket data");
        
        let user_mapping = json_storage_client.get_sprint_members().await?.unwrap_or(HashMap::new());
        info!("Have user mapping");
        
        let mut historical_data = json_storage_client.get_historical_data().await?.unwrap_or(CumulativeSprintContexts {
            history: Vec::new(),
        });

        let ticket_summary = fetch_client.fetch_ticket_summary(&self.sprint_context.name, &historical_data, previous_ticket_data, user_mapping).await?;
        info!("Have ticket summary");
        
        let trello_board_id = env::var("TRELLO_BOARD_ID").expect("TRELLO_BOARD_ID environment variable should exist");
        let board_link_block = context_block(&format!("<https://trello.com/b/{}|View sprint board>", trello_board_id));
        
        match self.sprint_command.as_str() {
            "/sprint-kickoff" => {
                let mut message_blocks = vec![
                    header_block(&format!("🔭 Sprint {} Preview: {} - {}", self.sprint_context.name, print_current_date(), self.sprint_context.end_date)),
                    section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(Some(&print_current_date()), &self.sprint_context.end_date)?))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.extend(
                    json_storage_client.get_historical_data().await?.unwrap_or_else(|| CumulativeSprintContexts {
                        history: Vec::new(),
                    })
                    .into_slack_blocks()
                );
                message_blocks.push(board_link_block);
                message_blocks.push(primary_button_block("Kick Off", "/sprint-kickoff-confirm",  &format!("{} {}", self.sprint_context.end_date, self.sprint_context.name)));
                Ok(message_blocks)
            },
            "/sprint-kickoff-confirm" => {
                json_storage_client.put_ticket_data(&(&ticket_summary).into()).await?;
                json_storage_client.put_sprint_data(&LiveSprintContext {
                    end_date: self.sprint_context.end_date.clone(),
                    name: self.sprint_context.name.clone(),
                    channel_id: self.sprint_context.channel_id.to_string(),
                    start_date: print_current_date(),
                    open_tickets_count_beginning: ticket_summary.open_ticket_count,
                    in_scope_tickets_count_beginning: ticket_summary.in_scope_ticket_count,
                    trello_board: env::var("TRELLO_BOARD_ID").expect("TRELLO_BOARD_ID environment variable should exist") //TODO: parameterize
                }).await?;
                let eventbridge_client = create_eventbridge_client().await;
                eventbridge_client.create_daily_trigger_rule(&self.sprint_context.name).await?;

                let mut message_blocks = vec![
                    header_block(&format!("🚀 Sprint {} Kickoff: {} - {}\nSprint starts now!", self.sprint_context.name, print_current_date(), self.sprint_context.end_date)),
                    section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(Some(&print_current_date()), &self.sprint_context.end_date)?))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.push(board_link_block);
                Ok(message_blocks)
            },
            "/sprint-check-in" => {
                let mut message_blocks = vec![
                    header_block(&format!("{} Sprint {} Check-In: {}", self.sprint_context.time_indicator(), self.sprint_context.name, print_current_date())),
                    section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", ticket_summary.open_ticket_count, ticket_summary.ticket_count, self.sprint_context.days_until_end())),
                    section_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.push(board_link_block);
                Ok(message_blocks)
            },
            "/daily-trigger" => {
                json_storage_client.put_ticket_data(&(&ticket_summary).into()).await?;

                let mut message_blocks = vec![
                    header_block(&format!("{} Daily Summary: {}", self.sprint_context.time_indicator(), print_current_date())),
                    section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", ticket_summary.open_ticket_count, ticket_summary.ticket_count, self.sprint_context.days_until_end())),
                    section_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.push(board_link_block);
                Ok(message_blocks)
            },
            "/sprint-review" => {
                let eventbridge_client = create_eventbridge_client().await;
                eventbridge_client.delete_daily_trigger_rule(&self.sprint_context.name).await?;
                let sprint_data = json_storage_client.get_sprint_data().await?.expect("ongoing sprint should exist");
                json_storage_client.clear_sprint_data().await?;

                let open_tickets_added_count = ticket_summary.open_ticket_count - sprint_data.open_tickets_count_beginning;
                let tickets_added_to_scope_count = ticket_summary.in_scope_ticket_count - sprint_data.in_scope_tickets_count_beginning;

                let mut message_blocks = vec![
                    header_block(&format!("🎆 Sprint {} Review: {} - {}", self.sprint_context.name, print_current_date(), self.sprint_context.end_date)),
                    header_block(&format!("\n*{}/{} Tickets* Completed in {} Days*", ticket_summary.completed_tickets.len(), ticket_summary.ticket_count, self.sprint_context.total_days())),
                    header_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage)),
                    header_block(&format!("\n{} tickets added to sprint", open_tickets_added_count)), //hide if 0 or show "removed" if negative
                    header_block(&format!("\n{} tickets added to scope", tickets_added_to_scope_count)), //hide if 0 or show "removed" if negative
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.extend(historical_data.into_slack_blocks());
                message_blocks.push(board_link_block);
                
                //add method to ticket_summary to remove completed and backlogged tickets, then push back into ticket_data.
                json_storage_client.put_ticket_data(&(&ticket_summary).into()).await?;

                historical_data.history.push(CumulativeSprintContext {
                    name: self.sprint_context.name.clone(),
                    start_date: self.sprint_context.start_date.clone(),
                    end_date: self.sprint_context.end_date.clone(),
                    percent_complete: ticket_summary.completed_percentage,
                    completed_tickets_count: ticket_summary.completed_tickets.len() as u32,
                    open_tickets_added_count,
                    tickets_added_to_scope_count
                });
                
                json_storage_client.put_historical_data(&historical_data).await?;

                Ok(message_blocks)
            },
            _ => Err(anyhow!("Unsupported command '{:?}'", self.sprint_command))
        }
    }
}