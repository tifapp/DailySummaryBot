mod ticket;
mod ticket_summary;
pub mod ticket_sources;
pub mod sprint_records;
pub mod events;
pub mod ticket_state;

use std::collections::{HashMap, VecDeque};
use std::env;
use std::ops::Deref;
use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde_json::Value;
use crate::utils::date::{days_between, print_current_date};
use crate::utils::eventbridge::{create_eventbridge_client, NotificationClient};
use crate::utils::slack_components::{context_block, header_block, primary_button_block, section_block};
use self::sprint_records::{
    ActiveSprintContext, CumulativeSprintContext, CumulativeSprintContexts, SprintClient,
};
use self::ticket_summary::TicketSummary;

#[derive(PartialEq, Debug)]
pub enum SprintCommand {
    SprintPreview{sprint_name: String, end_date: String, channel_id: String},
    SprintKickoff{sprint_name: String, end_date: String, channel_id: String},
    SprintCheckIn,
    SprintEnd,
    SprintCancel,
    DailySummary,
    SprintReview,
}

pub trait SprintCommandParser {
    async fn try_into_sprint_command(
        &self, 
        active_sprint_context: &Option<ActiveSprintContext>,
        cumulative_sprint_contexts: &CumulativeSprintContexts,
    ) -> Result<SprintCommand>;
}

impl ActiveSprintContext {
    pub fn days_until_end(&self) -> u32 {
        days_between(None, &self.end_date).expect("Days until end should be parseable") as u32
    }

    pub fn total_days(&self) -> u32 {
        days_between(Some(&self.start_date), &print_current_date()).expect("Total days should be parseable") as u32
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

pub fn count_difference(num1: i32, num2: i32) -> String {
    let diff = num1 - num2;
    if diff != 0 {
        let added_label = if diff < 0 {
            "removed from"
        } else {
            "added to"
        };

        format!("{} tickets {}", diff.abs(), added_label)
    } else {
        "No tickets added to".to_string()
    }
}

impl SprintCommand {
    pub async fn save_sprint_state(
        &self, 
        ticket_summary: &mut TicketSummary,
        active_sprint_context: &Option<ActiveSprintContext>,
        cumulative_sprint_contexts: &mut CumulativeSprintContexts,
        sprint_client: &dyn SprintClient,
        notification_client: &dyn NotificationClient
    ) -> Result<(), anyhow::Error> {
        sprint_client.put_ticket_data(&(ticket_summary).deref().into()).await?;
    
        match self {
            SprintCommand::SprintKickoff { sprint_name, end_date, channel_id } => {
                let new_sprint_context = ActiveSprintContext {
                    end_date: end_date.to_string(),
                    name: sprint_name.to_string(),
                    channel_id: channel_id.to_string(),
                    start_date: print_current_date(),
                    open_tickets_count_beginning: ticket_summary.open_ticket_count,
                    in_scope_tickets_count_beginning: ticket_summary.sprint_ticket_count,
                    trello_board: env::var("TRELLO_BOARD_ID")?,
                };
                sprint_client.put_sprint_data(&new_sprint_context).await?;
                notification_client.create_daily_trigger_rule(sprint_name).await?;
            },
            SprintCommand::SprintCancel | SprintCommand::SprintEnd | SprintCommand::SprintReview => {
                if let Some(sprint_data) = active_sprint_context {
                    notification_client.delete_daily_trigger_rule(&sprint_data.name).await?;
    
                    if matches!(self, SprintCommand::SprintEnd | SprintCommand::SprintReview) {
                        let open_tickets_added_count = ticket_summary.open_ticket_count as i32 - sprint_data.open_tickets_count_beginning as i32;
                        let tickets_added_to_scope_count = ticket_summary.sprint_ticket_count as i32 - sprint_data.in_scope_tickets_count_beginning as i32;

                        cumulative_sprint_contexts.history.push(CumulativeSprintContext {
                            name: sprint_data.name.clone(),
                            start_date: sprint_data.start_date.clone(),
                            end_date: sprint_data.end_date.clone(),
                            percent_complete: ticket_summary.completed_percentage,
                            completed_tickets_count: ticket_summary.completed_tickets.len() as u32,
                            open_tickets_added_count,
                            tickets_added_to_scope_count,
                        });

                        sprint_client.put_historical_data(cumulative_sprint_contexts).await?;
                        ticket_summary.clear_completed_and_deferred();
                    }
    
                    sprint_client.clear_sprint_data().await?;
                } else {
                    return Err(anyhow!("Active sprint context is required for this operation."));
                }
            },
            _ => {}
        }
    
        Ok(())
    }

    pub async fn create_sprint_message(
        &self, 
        ticket_summary: &TicketSummary,
        active_sprint_context: &Option<ActiveSprintContext>,
        cumulative_sprint_contexts: &CumulativeSprintContexts,
    ) -> Result<Vec<Value>> {
        let trello_board_id = env::var("TRELLO_BOARD_ID").unwrap_or("TRELLO_BOARD_ID needs to exist".to_owned());
        let board_link_block = context_block(&format!("<https://trello.com/b/{}|View sprint board>", trello_board_id));

        match self {
            SprintCommand::SprintPreview { sprint_name, end_date, channel_id: _ } => {
                Ok([
                    vec![
                        header_block(&format!("🔭 Sprint {} Preview: {} - {}", sprint_name, print_current_date(), end_date)),
                        section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(None, end_date)?)),
                    ],
                    cumulative_sprint_contexts.into_slack_blocks(),
                    ticket_summary.into_slack_blocks(),
                    vec![
                        board_link_block,
                        primary_button_block("Kick Off", "/sprint-kickoff-confirm",  &format!("{} {}", end_date, sprint_name))
                    ]
                    ].concat()
                )
            },
            SprintCommand::SprintKickoff { sprint_name, end_date, channel_id: _ } => {
                Ok([vec![
                        header_block(&format!("🚀 Sprint {} Kickoff: {} - {}", sprint_name, print_current_date(), end_date)),
                        section_block("\nSprint starts now!"),
                        section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(None, end_date)?)),
                    ],
                    ticket_summary.into_slack_blocks(),
                    vec![
                        board_link_block
                    ]]
                    .concat()
                )
            },
            SprintCommand::SprintCheckIn => {
                Ok([vec![
                    header_block(&format!("🛰️ Sprint {} Check-In: {}", active_sprint_context.as_ref().unwrap().name, print_current_date())),
                    section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", 
                        ticket_summary.open_ticket_count, 
                        ticket_summary.sprint_ticket_count, 
                        active_sprint_context.as_ref().unwrap().days_until_end()
                    )),
                    section_block(&format!("\n*{:.2}% of sprint scope completed.*", ticket_summary.completed_percentage)),
                ],
                    ticket_summary.into_slack_blocks(),
                vec![
                    board_link_block
                ]].concat())
            },
            SprintCommand::SprintCancel => {                
                Ok([vec![
                    header_block(&format!("🔴 Sprint {} is cancelled.", active_sprint_context.as_ref().unwrap().name)),
                    section_block(&format!("\n*{}/{} tickets completed in {} days.*", ticket_summary.completed_tickets.len(), ticket_summary.sprint_ticket_count, active_sprint_context.as_ref().unwrap().total_days())),
                    section_block(&format!("\n*{:.2}% of sprint scope completed.*\n", ticket_summary.completed_percentage)),
                    section_block("\nProgress will not be saved.\n"),
                ],
                    ticket_summary.into_slack_blocks(),
                vec![
                    board_link_block
                ]].concat())
            },
            SprintCommand::SprintEnd | SprintCommand::SprintReview => {
                let mut header = header_block(&format!("🎆 Sprint {} Review: {} - {}", active_sprint_context.as_ref().unwrap().name, print_current_date(), active_sprint_context.as_ref().unwrap().end_date));
                if self == &SprintCommand::SprintEnd {
                    header = header_block(&format!("💥 Sprint {} ended early.", active_sprint_context.as_ref().unwrap().name));
                }

                let completion_emoji = match ticket_summary.completed_percentage {
                    0.0..=10.0 => "🐢",
                    10.1..=25.0 => "🥉",
                    25.1..=50.0 => "🥈",
                    50.1..=75.0 => "🥇",
                    75.1..=90.0 => "🏅",
                    90.1..=99.9 => "🎖️",
                    100.0 => "🏆",
                    _ => "⁉️ What happened here?",
                };

                Ok([vec![
                        header,
                        section_block(&format!("\n*{}/{} tickets completed in {} days.*", ticket_summary.completed_tickets.len(), ticket_summary.sprint_ticket_count, active_sprint_context.as_ref().unwrap().total_days())),
                        section_block(&format!("\n*{:.2}% of sprint scope completed.*\n", ticket_summary.completed_percentage)),
                        header_block(completion_emoji),
                        section_block(&format!("\n{} this sprint.", count_difference(ticket_summary.open_ticket_count as i32, active_sprint_context.as_ref().unwrap().open_tickets_count_beginning as i32))),
                        section_block(&format!("\n{} project scope.", count_difference(ticket_summary.sprint_ticket_count as i32, active_sprint_context.as_ref().unwrap().in_scope_tickets_count_beginning as i32))),
                    ],
                    cumulative_sprint_contexts.into_slack_blocks(),
                    ticket_summary.into_slack_blocks(),
                    vec![
                        board_link_block
                    ]]
                    .concat()
                )
            },
            SprintCommand::DailySummary => {
                Ok([vec![
                    header_block(&format!("{} Daily Summary: {}", active_sprint_context.as_ref().unwrap().time_indicator(), print_current_date())),
                    section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", ticket_summary.open_ticket_count, ticket_summary.sprint_ticket_count, active_sprint_context.as_ref().unwrap().days_until_end())),
                    section_block(&format!("\n*{:.2}% of sprint scope completed.*", ticket_summary.completed_percentage)),
                ],
                    ticket_summary.into_slack_blocks(),
                 vec![   board_link_block
                ]].concat())
            }
        }
    }
}

#[cfg(test)]
mod sprint_event_message_generator_tests {
    use super::*;
    use crate::{sprint_summary::sprint_records::mocks::MockSprintClient, utils::eventbridge::eventbridge_mocks::MockEventBridgeClient};
    use sprint_event_message_generator_tests::sprint_records::ActiveSprintContextClient;
    use std::env;
    use tokio::runtime::Runtime;
    
    #[test]
    fn test_days_until_end() {
        let sprint_context = ActiveSprintContext {
            end_date: (chrono::Local::now() + chrono::Duration::days(10)).format("%m/%d/%y").to_string(),
            ..Default::default()
        };
        assert_eq!(sprint_context.days_until_end(), 10);
    }

    #[test]
    fn test_total_days() {
        let sprint_context = ActiveSprintContext {
            start_date: (chrono::Local::now() - chrono::Duration::days(5)).format("%m/%d/%y").to_string(),
            ..Default::default()
        };
        assert_eq!(sprint_context.total_days(), 5);
    }

    #[test]
    fn test_time_indicator() {
        let sprint_context = ActiveSprintContext {
            start_date: (chrono::Local::now() - chrono::Duration::days(10)).format("%m/%d/%y").to_string(),
            end_date: (chrono::Local::now() + chrono::Duration::days(10)).format("%m/%d/%y").to_string(),
            ..Default::default()
        };
        assert_eq!(sprint_context.time_indicator(), "🌕");

        let sprint_context_advanced = ActiveSprintContext {
            start_date: (chrono::Local::now() - chrono::Duration::days(30)).format("%m/%d/%y").to_string(),
            end_date: chrono::Local::now().format("%m/%d/%y").to_string(),
            ..Default::default()
        };
        assert_eq!(sprint_context_advanced.time_indicator(), "🌑");
    }

    fn test_runtime() -> Runtime {
        Runtime::new().unwrap()
    }
    
    #[test]
    fn test_sprint_preview_message() {
        let rt = test_runtime();
        let ticket_summary = TicketSummary::default();
        let cumulative_sprint_contexts = CumulativeSprintContexts::default();
        let active_sprint_context = ActiveSprintContext::default();
        let event = SprintCommand::SprintPreview {
            sprint_name: "My Sprint".to_string(),
            end_date: "12/31/23".to_string(),
            channel_id: "XYZ123".to_string(),
        };

        rt.block_on(async {
            let result = event.create_sprint_message(&ticket_summary, &Some(active_sprint_context), &cumulative_sprint_contexts).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Sprint Preview")));
            assert!(result.iter().any(|block| block.to_string().contains("View sprint board")));
        });
    }

    #[test]
    fn test_sprint_kickoff_saves_data() {
        env::set_var("TRELLO_BOARD_ID", "YourTrelloBoardID");
        let rt = test_runtime();
        let mut ticket_summary = TicketSummary::default();
        let mut cumulative_sprint_contexts = CumulativeSprintContexts::default();
        let mock_sprint_client = MockSprintClient::new(None, Some(cumulative_sprint_contexts.clone()), None);
        let mock_notification_client = MockEventBridgeClient::new();
        let event = SprintCommand::SprintKickoff {
            sprint_name: "New Sprint".to_string(),
            end_date: "12/31/23".to_string(),
            channel_id: "XYZ123".to_string(),
        };

        rt.block_on(async {
            let result = event.save_sprint_state(&mut ticket_summary, &None, &mut cumulative_sprint_contexts, &mock_sprint_client, &mock_notification_client).await.unwrap();
            assert!(mock_sprint_client.get_sprint_data().await.unwrap().is_some());
        });
    }
    
    #[test]
    fn test_sprint_review_with_historical_data() {
        let rt = test_runtime();
        env::set_var("TRELLO_BOARD_ID", "TestBoardID");
        let mut ticket_summary = TicketSummary::default();
        let active_sprint_context = ActiveSprintContext::default();
        let mut cumulative_sprint_contexts = CumulativeSprintContexts::default();
        let mock_sprint_client = MockSprintClient::new(Some(active_sprint_context.clone()), Some(cumulative_sprint_contexts.clone()), None);
        let mock_notification_client = MockEventBridgeClient::new();
        let event = SprintCommand::SprintReview;

        rt.block_on(async {
            mock_notification_client.create_daily_trigger_rule(&active_sprint_context.name).await.expect("Failed to create rule");
            let result = event.save_sprint_state( &mut ticket_summary,&Some(active_sprint_context),&mut cumulative_sprint_contexts, &mock_sprint_client,&mock_notification_client).await.unwrap();
            assert!(mock_sprint_client.get_sprint_data().await.unwrap().is_none());
        });
    }

    #[test]
    fn test_daily_summary_output() {
        let rt = test_runtime();
        let ticket_summary = TicketSummary::default();
        let active_sprint_context = ActiveSprintContext {
            end_date: (chrono::Local::now() + chrono::Duration::try_days(5).unwrap()).format("%m/%d/%y").to_string(),
            ..ActiveSprintContext::default()
        };
        let cumulative_sprint_contexts = CumulativeSprintContexts::default();
        let event = SprintCommand::DailySummary;

        rt.block_on(async {
            let result = event.create_sprint_message(&ticket_summary, &Some(active_sprint_context), &cumulative_sprint_contexts).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Daily Summary")));
            assert!(result.iter().any(|block| block.to_string().contains("Days* Remain In Sprint.")));
        });
    }
}
