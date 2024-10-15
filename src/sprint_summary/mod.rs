mod ticket;
mod ticket_summary;
pub mod ticket_sources;
pub mod sprint_records;
pub mod events;
pub mod ticket_state;
pub mod ticket_label;
use std::env;
use std::ops::Deref;
use anyhow::{Result, anyhow};
use serde_json::Value;
use crate::utils::date::{days_between, print_current_date};
use crate::utils::eventbridge::NotificationClient;
use crate::utils::slack_components::{context_block, header_block, primary_button_block, section_block};
use self::sprint_records::{
    ActiveSprintContext, CumulativeSprintContext, CumulativeSprintContexts, DailyTicketContexts, SprintClient
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

    pub fn total_days_elapsed(&self) -> u32 {
        days_between(Some(&self.start_date), &print_current_date()).expect("Total days should be parseable") as u32
    }
    
    pub fn remaining_time_indicator(&self) -> &str {
        let days_left = self.days_until_end() as f32;
        let total_days = days_between(Some(&self.start_date), &self.end_date).expect("Days should be parseable") as f32;
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

const DAILY_SUMMARY_TIME: &str = "cron(0 3 * * ? *)";
const SPRINT_REVIEW_TIME: &str = "cron(0 4 * * ? *)";

impl SprintCommand {
    pub async fn save_sprint_state(
        &self, 
        ticket_summary: &mut TicketSummary,
        active_sprint_context: &Option<ActiveSprintContext>,
        cumulative_sprint_contexts: &mut CumulativeSprintContexts,
        sprint_client: &dyn SprintClient,
        notification_client: &dyn NotificationClient
    ) -> Result<(), anyhow::Error> {    
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
                notification_client.create_daily_trigger_rule(sprint_name, DAILY_SUMMARY_TIME).await?;
                sprint_client.put_ticket_data(&(ticket_summary).deref().into()).await?;
            },
            SprintCommand::DailySummary => {
                sprint_client.put_ticket_data(&(ticket_summary).deref().into()).await?;
                let context = active_sprint_context.as_ref().unwrap();
                if (days_between(Some(&print_current_date()), &context.end_date).unwrap() == 1) {
                    notification_client.change_daily_trigger_rule(&context.name, SPRINT_REVIEW_TIME).await?;
                }
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
                        sprint_client.put_ticket_data(&(ticket_summary).deref().into()).await?;
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
        daily_ticket_contexts: &DailyTicketContexts
    ) -> Result<Vec<Value>> {
        let trello_board_id = env::var("TRELLO_BOARD_ID").unwrap_or("TRELLO_BOARD_ID needs to exist".to_owned());
        let project_scope_block = section_block(&format!("{} tickets left in project scope.", ticket_summary.project_ticket_count_in_scope));
        let board_link_block = context_block(&format!("<https://trello.com/b/{}|View sprint board>", trello_board_id));

        match self {
            SprintCommand::SprintPreview { sprint_name, end_date, channel_id: _ } => {
                Ok([
                    vec![
                        header_block(&format!("🔭 Sprint {} Preview: {} - {}", sprint_name, print_current_date(), end_date)),
                        section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(None, end_date)?)),
                        section_block(&format!("\n{} tickets will be carried over from last sprint.", daily_ticket_contexts.count_open_tickets())),
                    ],
                    cumulative_sprint_contexts.into_slack_blocks(),
                    ticket_summary.into_slack_blocks(),
                    vec![
                        project_scope_block,
                        board_link_block,
                        primary_button_block("Kick Off", "/sprint-kickoff-confirm",  &format!("{} {}", end_date, sprint_name)),
                    ]
                    ].concat()
                )
            },
            SprintCommand::SprintKickoff { sprint_name, end_date, channel_id: _ } => {
                Ok([
                    vec![
                        header_block(&format!("🚀 Sprint {} Kickoff: {} - {}", sprint_name, print_current_date(), end_date)),
                        section_block("\nSprint starts now!"),
                        section_block(&format!("*{} Tickets*\n*{:?} Days*", ticket_summary.open_ticket_count, days_between(None, end_date)?)),
                    ],
                    ticket_summary.into_slack_blocks(),
                    vec![
                        board_link_block
                    ]
                    ]
                    .concat()
                )
            },
            SprintCommand::SprintCheckIn => {
                Ok([vec![
                    header_block(&format!("🛰️ Sprint {} Check-In: {}", active_sprint_context.as_ref().unwrap().name, print_current_date())),
                    section_block(&format!("*{} tickets open* out of {}.\n*{} days* remain in sprint.", 
                        ticket_summary.open_ticket_count, 
                        ticket_summary.sprint_ticket_count, 
                        active_sprint_context.as_ref().unwrap().days_until_end()
                    )),
                    section_block(&format!("\n*{:.2}% of sprint scope completed.*", ticket_summary.completed_percentage)),
                ],
                    ticket_summary.into_slack_blocks(),
                vec![
                    project_scope_block,
                    board_link_block
                ]].concat())
            },
            SprintCommand::SprintCancel => {                
                Ok([vec![
                    header_block(&format!("🔴 Sprint {} is cancelled.", active_sprint_context.as_ref().unwrap().name)),
                    section_block(&format!("\n*{}/{} tickets completed in {} days.*", ticket_summary.completed_tickets.len(), ticket_summary.sprint_ticket_count, active_sprint_context.as_ref().unwrap().total_days_elapsed())),
                    section_block(&format!("\n*{:.2}% of sprint scope completed.*\n", ticket_summary.completed_percentage)),
                    section_block("\nProgress will not be saved.\n"),
                ],
                    ticket_summary.into_slack_blocks(),
                vec![
                    project_scope_block,
                    board_link_block,
                ]].concat())
            },
            SprintCommand::SprintEnd | SprintCommand::SprintReview => {
                let mut header = header_block(&format!("🎆 Sprint {} Review: {} - {}", active_sprint_context.as_ref().unwrap().name, active_sprint_context.as_ref().unwrap().start_date, active_sprint_context.as_ref().unwrap().end_date));
                if self == &SprintCommand::SprintEnd {
                    header = header_block(&format!("💥 Sprint {} ended early.", active_sprint_context.as_ref().unwrap().name));
                }
                
                let completion_emoji = if (0.0..25.0).contains(&ticket_summary.completed_percentage) {
                    "🐢 League Entrants"
                } else if (25.0..50.0).contains(&ticket_summary.completed_percentage) {
                    "🥉 Local Competitors"
                } else if (50.0..65.0).contains(&ticket_summary.completed_percentage) {
                    "🥈 Playoff Contenders"
                } else if (65.0..80.0).contains(&ticket_summary.completed_percentage) {
                    "🥇 Division Leaders"
                } else if (80.0..90.0).contains(&ticket_summary.completed_percentage) {
                    "🏅 Conference Finalists"
                } else if (90.0..100.0).contains(&ticket_summary.completed_percentage) {
                    "🎖️ World Series Elites"
                } else if ticket_summary.completed_percentage == 100.0 {
                    "🏆 Sprint Champions!"
                } else {
                    "⁉️ Disqualified?"
                };                

                Ok([vec![
                        header,
                        section_block(&format!("\n*{}/{} tickets completed in {} days.*", ticket_summary.completed_tickets.len(), ticket_summary.sprint_ticket_count, active_sprint_context.as_ref().unwrap().total_days_elapsed())),
                        section_block(&format!("\n*{:.2}% of sprint scope completed.*\n", ticket_summary.completed_percentage)),
                        header_block(completion_emoji),
                    ],
                    cumulative_sprint_contexts.into_slack_blocks(),
                    ticket_summary.into_slack_blocks(),
                    vec![
                        section_block(&format!("\n{} this sprint.", count_difference(ticket_summary.open_ticket_count as i32, active_sprint_context.as_ref().unwrap().open_tickets_count_beginning as i32))),
                        section_block(&format!("\n{} project scope.", count_difference(ticket_summary.sprint_ticket_count as i32, active_sprint_context.as_ref().unwrap().in_scope_tickets_count_beginning as i32))),
                        project_scope_block,
                        board_link_block
                    ]]
                    .concat()
                )
            },
            SprintCommand::DailySummary => {
                Ok([
                    vec![
                        header_block(&format!("{} Daily Summary: {}", active_sprint_context.as_ref().unwrap().remaining_time_indicator(), print_current_date())),
                        section_block(&format!("*{} tickets open* out of {}.\n*{} days* remain in sprint.", 
                            ticket_summary.open_ticket_count, 
                            ticket_summary.sprint_ticket_count, 
                            active_sprint_context.as_ref().unwrap().days_until_end()
                        )),
                        section_block(&format!("\n*{:.2}% of sprint scope completed.*", ticket_summary.completed_percentage)),
                    ],
                    ticket_summary.into_slack_blocks(),
                    vec![   
                        board_link_block,
                        section_block(&format!("{} tickets left in project scope.", ticket_summary.project_ticket_count_in_scope)),
                    ]
                ].concat())
            }
        }
    }
}

#[cfg(test)]
mod sprint_event_message_generator_tests {
    use super::*;
    use crate::{sprint_summary::sprint_records::mocks::MockSprintClient, utils::eventbridge::eventbridge_mocks::MockEventBridgeClient};
    use chrono_tz::US::Pacific;
    use sprint_event_message_generator_tests::sprint_records::{ActiveSprintContextClient, DailyTicketContextClient};
    use std::env;
    use tokio::runtime::Runtime;
    
    #[test]
    fn test_days_until_end() {
        let sprint_context = ActiveSprintContext {
            end_date: (chrono::Local::now().with_timezone(&Pacific) + chrono::Duration::try_days(10).unwrap()).format("%m/%d/%y").to_string(),
            ..Default::default()
        };
        assert_eq!(sprint_context.days_until_end(), 10);
    }

    #[test]
    fn test_total_days_elapsed() {
        let sprint_context = ActiveSprintContext {
            start_date: (chrono::Local::now().with_timezone(&Pacific) - chrono::Duration::try_days(5).unwrap()).format("%m/%d/%y").to_string(),
            ..Default::default()
        };
        assert_eq!(sprint_context.total_days_elapsed(), 5);
    }

    #[test]
    fn test_remaining_time_indicator() {
        let sprint_context = ActiveSprintContext {
            start_date: (chrono::Local::now().with_timezone(&Pacific) - chrono::Duration::try_days(10).unwrap()).format("%m/%d/%y").to_string(),
            end_date: (chrono::Local::now().with_timezone(&Pacific) + chrono::Duration::try_days(10).unwrap()).format("%m/%d/%y").to_string(),
            ..Default::default()
        };
        assert_eq!(sprint_context.remaining_time_indicator(), "🌓");

        let sprint_context_advanced = ActiveSprintContext {
            start_date: (chrono::Local::now().with_timezone(&Pacific) - chrono::Duration::try_days(30).unwrap()).format("%m/%d/%y").to_string(),
            end_date: chrono::Local::now().with_timezone(&Pacific).format("%m/%d/%y").to_string(),
            ..Default::default()
        };
        assert_eq!(sprint_context_advanced.remaining_time_indicator(), "🌑");
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
        let daily_ticket_contexts = DailyTicketContexts::default();
        let event = SprintCommand::SprintPreview {
            sprint_name: "My Sprint".to_string(),
            end_date: "12/31/23".to_string(),
            channel_id: "XYZ123".to_string(),
        };

        rt.block_on(async {
            let result = event.create_sprint_message(&ticket_summary, &Some(active_sprint_context), &cumulative_sprint_contexts, &daily_ticket_contexts).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Sprint Preview")));
            assert!(result.iter().any(|block| block.to_string().contains("View sprint board")));
            assert!(result.iter().any(|block| block.to_string().contains("tickets will be carried over from last sprint.")));
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
        let end_date = (chrono::Local::now().with_timezone(&Pacific) + chrono::Duration::try_days(10).unwrap()).date_naive().format("%m/%d/%y").to_string();
        let event = SprintCommand::SprintKickoff {
            sprint_name: "New Sprint".to_string(),
            end_date: end_date.clone(),
            channel_id: "XYZ123".to_string(),
        };

        rt.block_on(async {
            let _ = event.save_sprint_state(&mut ticket_summary, &None, &mut cumulative_sprint_contexts, &mock_sprint_client, &mock_notification_client).await.unwrap();
            assert_eq!(mock_sprint_client.get_sprint_data().await.unwrap().unwrap(), ActiveSprintContext { 
                name: "New Sprint".to_string(), 
                start_date: print_current_date(), 
                end_date, 
                channel_id: "XYZ123".to_string(), 
                trello_board: "YourTrelloBoardID".to_string(), 
                open_tickets_count_beginning: 20, 
                in_scope_tickets_count_beginning: 15
            });
        });
    }

    #[test]
    fn test_sprint_cancel_clears_data() {
        let rt = test_runtime();
        let mut ticket_summary = TicketSummary::default();
        let mut cumulative_sprint_contexts = CumulativeSprintContexts::default();
        let active_sprint_context = Some(ActiveSprintContext::default());
        let mock_sprint_client = MockSprintClient::new(active_sprint_context.clone(), Some(cumulative_sprint_contexts.clone()), None);
        let mock_notification_client = MockEventBridgeClient::new();
        let event = SprintCommand::SprintCancel;

        rt.block_on(async {
            let _ = mock_notification_client.create_daily_trigger_rule("Sprint 1", DAILY_SUMMARY_TIME).await;
            let _ = event.save_sprint_state(&mut ticket_summary, &active_sprint_context.clone(), &mut cumulative_sprint_contexts, &mock_sprint_client, &mock_notification_client).await.unwrap();
            assert_eq!(mock_sprint_client.get_sprint_data().await.unwrap(), None);
        });
    }

    #[test]
    fn test_daily_summary_saves_ticket_data() {
        let rt = test_runtime();
        let mut ticket_summary = TicketSummary::default();
        let mock_sprint_client = MockSprintClient::new(None, None, None);
        let mock_notification_client = MockEventBridgeClient::new();
        let event = SprintCommand::DailySummary;

        rt.block_on(async {
            let _ = event.save_sprint_state(&mut ticket_summary, &None, &mut CumulativeSprintContexts::default(), &mock_sprint_client, &mock_notification_client).await.unwrap();
            assert!(mock_sprint_client.get_ticket_data().await.unwrap().is_some());
        });
    }
    
    #[test]
    fn test_daily_summary_updates_trigger_rule_before_deadline() {
        let rt = test_runtime();
        let mut ticket_summary = TicketSummary::default();
        let active_sprint_context = ActiveSprintContext {
            end_date: (chrono::Local::now().with_timezone(&Pacific) + chrono::Duration::try_days(1).unwrap()).format("%m/%d/%y").to_string(),
            ..ActiveSprintContext::default()
        };
        let mock_sprint_client = MockSprintClient::new(None, None, None);
        let mock_notification_client = MockEventBridgeClient::new();
        let action = SprintCommand::DailySummary;

        rt.block_on(async {
            let name = &active_sprint_context.name.clone();
            let _ = mock_notification_client.create_daily_trigger_rule(name, DAILY_SUMMARY_TIME).await;
            let _ = action.save_sprint_state(&mut ticket_summary, &Some(active_sprint_context), &mut CumulativeSprintContexts::default(), &mock_sprint_client, &mock_notification_client).await.unwrap();
            assert!(mock_notification_client.rules_created.lock().await.get(name) == Some(&SPRINT_REVIEW_TIME.to_string()));
        });
    }
    
    #[test]
    fn test_sprint_review_clears_current_sprint_data() {
        let rt = test_runtime();
        env::set_var("TRELLO_BOARD_ID", "TestBoardID");
        let mut ticket_summary = TicketSummary::default();
        let active_sprint_context = ActiveSprintContext::default();
        let mut cumulative_sprint_contexts = CumulativeSprintContexts::default();
        let mock_sprint_client = MockSprintClient::new(Some(active_sprint_context.clone()), Some(cumulative_sprint_contexts.clone()), None);
        let mock_notification_client = MockEventBridgeClient::new();
        let action = SprintCommand::SprintReview;

        rt.block_on(async {
            let name = &active_sprint_context.name.clone();
            let _ = mock_notification_client.create_daily_trigger_rule(name, DAILY_SUMMARY_TIME).await;
            let _ = action.save_sprint_state( &mut ticket_summary,&Some(active_sprint_context),&mut cumulative_sprint_contexts, &mock_sprint_client,&mock_notification_client).await.unwrap();
            assert!(mock_sprint_client.get_sprint_data().await.unwrap().is_none());        
            assert!(mock_notification_client.rules_deleted.lock().await.contains(name));
        });
    }
    
    #[test]
    fn test_sprint_review_message() {
        let rt = test_runtime();
        env::set_var("TRELLO_BOARD_ID", "TestBoardID");
        let ticket_summary = TicketSummary::default();
        let cumulative_sprint_contexts = CumulativeSprintContexts::default();
        let daily_ticket_contexts = DailyTicketContexts::default();
        let action = SprintCommand::SprintReview;
        
        let mut active_sprint_context = ActiveSprintContext::default();
        active_sprint_context.name = "21-Pascal".to_string();
        active_sprint_context.end_date = "05/28/24".to_string();
        active_sprint_context.end_date = "06/11/24".to_string();

        rt.block_on(async {
            let result = action.create_sprint_message(&ticket_summary, &Some(active_sprint_context), &cumulative_sprint_contexts, &daily_ticket_contexts).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Sprint 21-Pascal Review: 05/28/24 - 06/11/24")));
            assert!(result.iter().any(|block| block.to_string().contains("completed in 14 days.")));
            assert!(result.iter().any(|block| block.to_string().contains("of sprint scope completed.")));
        });
    }

    #[test]
    fn test_daily_summary_message() {
        let rt = test_runtime();
        let ticket_summary = TicketSummary::default();
        let active_sprint_context = ActiveSprintContext {
            end_date: (chrono::Local::now().with_timezone(&Pacific) + chrono::Duration::try_days(5).unwrap()).format("%m/%d/%y").to_string(),
            ..ActiveSprintContext::default()
        };
        let cumulative_sprint_contexts = CumulativeSprintContexts::default();
        let event = SprintCommand::DailySummary;
        let daily_ticket_contexts = DailyTicketContexts::default();

        rt.block_on(async {
            let result = event.create_sprint_message(&ticket_summary, &Some(active_sprint_context), &cumulative_sprint_contexts, &daily_ticket_contexts).await.unwrap();
            assert!(result.iter().any(|block| block.to_string().contains("Daily Summary")));
            assert!(result.iter().any(|block| block.to_string().contains("tickets open* out of")));
            assert!(result.iter().any(|block| block.to_string().contains("5 days* remain in sprint.")));
        });
    }
}
