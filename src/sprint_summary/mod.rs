mod ticket;
mod ticket_summary;
mod ticket_sources;
mod sprint_records;
mod events;
pub mod ticket_state;

use std::collections::{HashMap, VecDeque};
use std::env;
use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use crate::utils::date::{days_between, print_current_date};
use crate::utils::eventbridge::{create_eventbridge_client, EventBridgeExtensions};
use crate::utils::slack_components::{context_block, header_block, primary_button_block, section_block};
use self::sprint_records::{
    ActiveSprintContext, CumulativeSprintContext, CumulativeSprintContexts, DailyTicketContexts, SprintClient,
};
use self::ticket_sources::TicketSummaryClient;

#[derive(Debug, Deserialize, Clone)]
pub struct SprintContext {
    pub channel_id: String,
    pub start_date: String,
    pub end_date: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SprintEvent {
    pub sprint_command: String,
    pub response_url: Option<String>,
    pub sprint_context: SprintContext,
}

pub trait SprintEventParser {
    async fn try_into_sprint_event<'a>(
        &self, 
        sprint_client: &'a dyn SprintClient
    ) -> Result<SprintEvent>;
}

impl SprintContext {
    //add unit test
    pub fn days_until_end(&self) -> u32 {
        days_between(None, &self.end_date).expect("Days until end should be parseable") as u32
    }

    //add unit test
    pub fn total_days(&self) -> u32 {
        days_between(Some(&self.start_date), &self.end_date).expect("Total days should be parseable") as u32
    }
    
    //add unit test
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
    async fn create_sprint_event_message<'a>(
        &self, 
        fetch_client: &dyn TicketSummaryClient,
        sprint_client: &'a dyn SprintClient
    ) -> Result<Vec<Value>>;
}

impl SprintEventMessageGenerator for SprintEvent {
    async fn create_sprint_event_message<'a>(
        &self, 
        fetch_client: &dyn TicketSummaryClient,
        sprint_client: &'a dyn SprintClient
    ) -> Result<Vec<Value>> {      
        let previous_ticket_data = sprint_client.get_ticket_data().await?.unwrap_or(DailyTicketContexts {
            tickets: VecDeque::new(),
        });
        
        let user_mapping = sprint_client.get_sprint_members().await?.unwrap_or(HashMap::new());
        
        let mut historical_data = sprint_client.get_historical_data().await?.unwrap_or(CumulativeSprintContexts {
            history: Vec::new(),
        });

        let mut ticket_summary = fetch_client.fetch_ticket_summary(&self.sprint_context.name, &historical_data, previous_ticket_data, user_mapping).await?;
        
        let trello_board_id = env::var("TRELLO_BOARD_ID").unwrap_or("TRELLO_BOARD_ID needs to exist".to_owned());
        let board_link_block = context_block(&format!("<https://trello.com/b/{}|View sprint board>", trello_board_id));
        
        //add unit test for running multiple commands in sequence
        match self.sprint_command.as_str() {
            "/sprint-kickoff" => {
                //add a unit test to validate the output message
                let mut message_blocks = vec![
                    header_block(&format!("🔭 Sprint {} Preview: {} - {}", self.sprint_context.name, print_current_date(), self.sprint_context.end_date)),
                    section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(Some(&print_current_date()), &self.sprint_context.end_date)?))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.extend(
                    sprint_client.get_historical_data().await?.unwrap_or_else(|| CumulativeSprintContexts {
                        history: Vec::new(),
                    })
                    .into_slack_blocks()
                );
                message_blocks.push(board_link_block);
                message_blocks.push(primary_button_block("Kick Off", "/sprint-kickoff-confirm",  &format!("{} {}", self.sprint_context.end_date, self.sprint_context.name)));
                Ok(message_blocks)
            },
            "/sprint-kickoff-confirm" => {
                //add a unit test to validate the output message
                //validate that data is saved to the mock hashmaps
                sprint_client.put_ticket_data(&(&ticket_summary).into()).await?;
                sprint_client.put_sprint_data(&ActiveSprintContext {
                    end_date: self.sprint_context.end_date.clone(),
                    name: self.sprint_context.name.clone(),
                    channel_id: self.sprint_context.channel_id.to_string(),
                    start_date: print_current_date(),
                    open_tickets_count_beginning: ticket_summary.open_ticket_count,
                    in_scope_tickets_count_beginning: ticket_summary.in_sprint_scope_ticket_count,
                    trello_board: env::var("TRELLO_BOARD_ID").expect("TRELLO_BOARD_ID environment variable should exist") //TODO: parameterize
                }).await?;
                let eventbridge_client = create_eventbridge_client().await;
                eventbridge_client.create_daily_trigger_rule(&self.sprint_context.name).await?;

                let mut message_blocks = vec![
                    header_block(&format!("🚀 Sprint {} Kickoff: {} - {}", self.sprint_context.name, print_current_date(), self.sprint_context.end_date)),
                    section_block("\nSprint starts now!"),
                    section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(Some(&print_current_date()), &self.sprint_context.end_date)?))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.push(board_link_block);
                Ok(message_blocks)
            },
            "/sprint-check-in" => {
                //add a unit test to validate the output message
                let mut message_blocks = vec![
                    header_block(&format!("{} Sprint {} Check-In: {}", self.sprint_context.time_indicator(), self.sprint_context.name, print_current_date())),
                    section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", ticket_summary.open_ticket_count, ticket_summary.in_sprint_scope_ticket_count, self.sprint_context.days_until_end())),
                    section_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.push(board_link_block);
                Ok(message_blocks)
            },
            "/daily-summary" => {
                //add a unit test to validate the output message
                //validate that data is saved to the mock hashmap
                sprint_client.put_ticket_data(&(&ticket_summary).into()).await?;

                let mut message_blocks = vec![
                    header_block(&format!("{} Daily Summary: {}", self.sprint_context.time_indicator(), print_current_date())),
                    section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", ticket_summary.open_ticket_count, ticket_summary.in_sprint_scope_ticket_count, self.sprint_context.days_until_end())),
                    section_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage))
                ];
                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.push(board_link_block);
                //sdend trtello request to change board name
                Ok(message_blocks)
            },
            "/sprint-review" => {
                //add a unit test to validate the output message with/without mock historical data
                //validate that data is saved to the mock hashmaps
                let eventbridge_client = create_eventbridge_client().await;
                eventbridge_client.delete_daily_trigger_rule(&self.sprint_context.name).await?;
                let sprint_data = sprint_client.get_sprint_data().await?.expect("ongoing sprint should exist");
                sprint_client.clear_sprint_data().await?;

                let open_tickets_added_count = ticket_summary.open_ticket_count as i32 - sprint_data.open_tickets_count_beginning as i32;
                let tickets_added_to_scope_count = ticket_summary.in_sprint_scope_ticket_count as i32 - sprint_data.in_scope_tickets_count_beginning as i32;

                let mut message_blocks = vec![
                    header_block(&format!("🎆 Sprint {} Review: {} - {}", self.sprint_context.name, print_current_date(), self.sprint_context.end_date)),
                    header_block(&format!("\n*{}/{} Tickets* Completed in {} Days*", ticket_summary.completed_tickets.len(), ticket_summary.in_sprint_scope_ticket_count, self.sprint_context.total_days())),
                    header_block(&format!("\n*{:.2}% of tasks completed.*", ticket_summary.completed_percentage)),
                ];
                
                if open_tickets_added_count != 0 {
                    let tickets_added_label = if open_tickets_added_count < 0 {
                        "removed"
                    } else {
                        "added to sprint"
                    };
                    message_blocks.push(header_block(&format!("\n{} tickets {}", open_tickets_added_count.abs(), tickets_added_label)));
                }

                if tickets_added_to_scope_count != 0 {
                    let tickets_added_scope_label = if tickets_added_to_scope_count < 0 {
                        "removed from scope"
                    } else {
                        "added to scope"
                    };
                    message_blocks.push(header_block(&format!("\n{} tickets {}", tickets_added_to_scope_count.abs(), tickets_added_scope_label)));
                }

                message_blocks.extend(ticket_summary.into_slack_blocks());
                message_blocks.extend(historical_data.into_slack_blocks());
                message_blocks.push(board_link_block);

                historical_data.history.push(CumulativeSprintContext {
                    name: self.sprint_context.name.clone(),
                    start_date: self.sprint_context.start_date.clone(),
                    end_date: self.sprint_context.end_date.clone(),
                    percent_complete: ticket_summary.completed_percentage,
                    completed_tickets_count: ticket_summary.completed_tickets.len() as u32,
                    open_tickets_added_count,
                    tickets_added_to_scope_count
                });
                
                sprint_client.put_historical_data(&historical_data).await?;
                
                //send request to trello to move completed tickets to garbage bin
                ticket_summary.clear_completed_and_deferred();
                sprint_client.put_ticket_data(&(&ticket_summary).into()).await?;

                Ok(message_blocks)
            },
            _ => Err(anyhow!("Unsupported command '{:?}'", self.sprint_command))
        }
    }
}

#[cfg(test)]
mod sprint_event_message_generator_tests {
    use self::ticket_sources::ticket_summary_mocks::{MockPullRequestClient, MockTicketDetailsClient, MockTicketSummaryClient};

    use super::*;
    use crate::sprint_summary::sprint_records::mocks::MockSprintClient;
    use sprint_event_message_generator_tests::sprint_records::ActiveSprintContextClient;
    use std::env;
    use tokio::runtime::Runtime;
    
    impl Default for SprintContext {
        fn default() -> Self {
            SprintContext {
                channel_id: "AKJFFKOL".to_string(),
                start_date: "02/20/23".to_string(),
                end_date: "02/30/23".to_string(),
                name: "test sprint 190".to_string()
            }
        }
    }

    // Helper function to create a runtime to execute async tests
    fn test_runtime() -> Runtime {
        Runtime::new().unwrap()
    }

    #[test]
    fn test_sprint_kickoff_message() {
        let rt = test_runtime();
        let client = MockTicketSummaryClient::new(
            MockTicketDetailsClient::new(vec![]),
            MockPullRequestClient::new(HashMap::new())
        );
        let mock_client = MockSprintClient::new(Some(ActiveSprintContext::default()), Some(CumulativeSprintContexts::default()), None);
        let event = SprintEvent {
            sprint_command: "/sprint-kickoff".to_string(),
            response_url: None,
            sprint_context: SprintContext::default(),
        };

        rt.block_on(async {
            let result = event.create_sprint_event_message(&client, &mock_client).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Sprint Kickoff")));
            assert!(result.iter().any(|block| block.to_string().contains("View sprint board")));
        });
    }

    #[test]
    fn test_sprint_kickoff_confirm_saves_data() {
        let rt = test_runtime();
        let client = MockTicketSummaryClient::new(
            MockTicketDetailsClient::new(vec![]),
            MockPullRequestClient::new(HashMap::new())
        );
        let mock_client = MockSprintClient::new(None, None, None);
        let event = SprintEvent {
            sprint_command: "/sprint-kickoff-confirm".to_string(),
            response_url: None,
            sprint_context: SprintContext::default(),
        };

        rt.block_on(async {
            let result = event.create_sprint_event_message(&client, &mock_client).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Sprint starts now!")));
            assert!(mock_client.get_sprint_data().await.unwrap().is_some());
        });
    }

    #[test]
    fn test_sprint_review_with_historical_data() {
        let rt = test_runtime();
        env::set_var("TRELLO_BOARD_ID", "TestBoardID");
        let client = MockTicketSummaryClient::new(
            MockTicketDetailsClient::new(vec![]),
            MockPullRequestClient::new(HashMap::new())
        );
        let mock_client = MockSprintClient::new(Some(ActiveSprintContext::default()), None, None);
        let event = SprintEvent {
            sprint_command: "/sprint-review".to_string(),
            response_url: None,
            sprint_context: SprintContext::default(),
        };

        rt.block_on(async {
            let result = event.create_sprint_event_message(&client, &mock_client).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Sprint Review:")));
            assert!(result.iter().any(|block| block.to_string().contains("tickets added to sprint")));
            assert!(mock_client.get_sprint_data().await.unwrap().is_none());
        });
    }

    #[test]
    fn test_daily_summary_output() {
        let rt = test_runtime();
        let client = MockTicketSummaryClient::new(
            MockTicketDetailsClient::new(vec![]),
            MockPullRequestClient::new(HashMap::new())
        );
        let mock_client = MockSprintClient::new(Some(ActiveSprintContext {
            name: "Sprint 101".to_string(),
            start_date: "2023-01-01".to_string(),
            end_date: "2023-01-15".to_string(),
            channel_id: "C123456".to_string(),
            trello_board: "Board123".to_string(),
            open_tickets_count_beginning: 10,
            in_scope_tickets_count_beginning: 5,
        }), None, None);
        let event = SprintEvent {
            sprint_command: "/daily-summary".to_string(),
            response_url: None,
            sprint_context: SprintContext::default(),
        };

        rt.block_on(async {
            let result = event.create_sprint_event_message(&client, &mock_client).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Daily Summary")));
        });
    }
}
