use std::{self, collections::VecDeque};
use std::collections::HashMap;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{from_value, Value};

use crate::utils::s3::JsonStorageClient;
use crate::utils::slack_components::section_block;

use super::ticket_state::TicketState;

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

//Sprint record is updated at the beginning of each sprint
#[derive(Debug, Serialize, Deserialize)]
pub struct ActiveSprintContext {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub channel_id: String,
    pub trello_board: String,
    pub open_tickets_count_beginning: u32,
    pub in_scope_tickets_count_beginning: u32,
}

pub trait ActiveSprintContextClient {
    async fn get_sprint_data(&self) -> Result<Option<ActiveSprintContext>>;
    async fn put_sprint_data(&self, sprint_record: &ActiveSprintContext) -> Result<()>;
    async fn clear_sprint_data(&self) -> Result<()>;
}

impl<T> ActiveSprintContextClient for T where T: JsonStorageClient, {
    async fn get_sprint_data(&self) -> Result<Option<ActiveSprintContext>> {
        self.get_json("sprint_data.json").await?
            .map(|json_value| {
                from_value::<ActiveSprintContext>(json_value)
                    .context("Failed to deserialize sprint data")
            })
            .transpose()
    }
    
    async fn put_sprint_data(&self, sprint_data: &ActiveSprintContext) -> Result<()> {
        let sprint_data_value = serde_json::to_value(sprint_data)
            .context("Failed to convert ticket data to JSON value")?;
    
        self.put_json("sprint_data.json", &sprint_data_value).await
    }
    
    async fn clear_sprint_data(&self) -> Result<()> {
        self.delete_json("sprint_data.json").await
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyTicketContext {
    pub id: String,
    pub name: String, 
    pub url: String,
    pub state: TicketState,
    pub is_goal: bool,
    pub added_on: String,
    pub added_in_sprint: String,
    pub last_moved_on: String,
}

//Ticket records update daily
#[derive(Debug, Serialize, Deserialize)]
pub struct DailyTicketContexts {
    pub tickets: VecDeque<DailyTicketContext>,
}

pub trait DailyTicketContextClient {
    async fn get_ticket_data(&self) -> Result<Option<DailyTicketContexts>>;
    async fn put_ticket_data(&self, ticket_data: &DailyTicketContexts) -> Result<()>;
}

impl<T> DailyTicketContextClient for T where T: JsonStorageClient, {
    async fn get_ticket_data(&self) -> Result<Option<DailyTicketContexts>> {
        self.get_json("ticket_data.json").await?
            .map(|json_value| {
                from_value::<DailyTicketContexts>(json_value)
                    .context("Failed to deserialize sprint data")
            })
            .transpose()
    }

    async fn put_ticket_data(&self, ticket_data: &DailyTicketContexts) -> Result<()> {
        let ticket_data_value = serde_json::to_value(ticket_data)
            .context("Failed to convert ticket data to JSON value")?;

        self.put_json("ticket_data.json", &ticket_data_value).await
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CumulativeSprintContext {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub percent_complete: f64,
    pub completed_tickets_count: u32,
    pub tickets_added_to_scope_count: u32,
    pub open_tickets_added_count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CumulativeSprintContexts {
    pub history: Vec<CumulativeSprintContext>
}

impl CumulativeSprintContexts {
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
                record.completed_tickets_count,
                record.percent_complete
            ))
        }));

        blocks
    }

    pub fn count_sprints_since(&self, sprint_name: &str) -> usize {
        self.history
            .iter()
            .rev()
            .position(|item| item.name == sprint_name)
            .map(|index| index + 1)
            .unwrap_or(0)
    }

    pub fn was_sprint_name_used(&self, sprint_name: &str) -> bool {
        self.count_sprints_since(sprint_name) > 0
    }
}

//Historical record is updated at the end of each sprint, keeping a running tally of sprint progress
pub trait CumulativeSprintContextClient {
    async fn get_historical_data(&self) -> Result<Option<CumulativeSprintContexts>>;
    async fn put_historical_data(&self, ticket_data: &CumulativeSprintContexts) -> Result<()>;
}

impl<T> CumulativeSprintContextClient for T where T: JsonStorageClient, {
    async fn get_historical_data(&self) -> Result<Option<CumulativeSprintContexts>> {
        self.get_json("historical_data.json").await?
            .map(|json_value| {
                from_value::<CumulativeSprintContexts>(json_value)
                    .context("Failed to deserialize sprint data")
            })
            .transpose()
    }
    
    async fn put_historical_data(&self, historical_data: &CumulativeSprintContexts) -> Result<()> {
        let historical_data_value = serde_json::to_value(historical_data)
            .context("Failed to convert historical data to JSON value")?;
    
        self.put_json("historical_data.json", &historical_data_value).await
    }
}

#[cfg(test)]
pub mod mocks {
    use std::collections::VecDeque;

    use crate::sprint_summary::ticket_state::TicketState;
    use super::{CumulativeSprintContext, CumulativeSprintContexts, DailyTicketContext, DailyTicketContexts};
    
    impl Default for CumulativeSprintContext {
        fn default() -> Self {
            CumulativeSprintContext { 
                name: "Sprint 101".to_string(), 
                start_date: "01/01/24".to_string(), 
                end_date: "02/01/24".to_string(), 
                percent_complete: 0.9, 
                completed_tickets_count: 12, 
                tickets_added_to_scope_count: 5, 
                open_tickets_added_count: 7 
            }
        }
    }
    
    impl Default for CumulativeSprintContexts {
        fn default() -> Self {
            CumulativeSprintContexts {
                history: vec![
                    CumulativeSprintContext { ..CumulativeSprintContext::default() },
                    CumulativeSprintContext { name: "Sprint 102".to_string(), ..CumulativeSprintContext::default() },
                ]
            }
        }
    }

    impl Default for DailyTicketContext {
        fn default() -> Self {
            DailyTicketContext {
                id: "abc123".to_string(),
                added_on: "04/01/24".to_string(),
                last_moved_on: "04/05/24".to_string(),
                added_in_sprint: "Sprint 101".to_string(),
                state: TicketState::Done,
                name: "Recorded Ticket".to_string(),
                url: "http://example.com/ticket2".to_string(),
                is_goal: false,
            }
        }
    }
    
    impl Default for DailyTicketContexts {
        fn default() -> Self {
            DailyTicketContexts {
                tickets: VecDeque::from(vec![
                    DailyTicketContext { ..DailyTicketContext::default() },
                    DailyTicketContext { name: "Recorded Ticket 2".to_string(), id: "abc456".to_string(), ..DailyTicketContext::default() },
                ])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_into_slack_blocks_empty_history() {
        let contexts = CumulativeSprintContexts { history: vec![] };
        assert!(contexts.into_slack_blocks().is_empty());
    }

    #[test]
    fn test_into_slack_blocks_existing_history() {
        let contexts = CumulativeSprintContexts {
            history: vec![
                CumulativeSprintContext {
                    name: "Sprint 1".to_string(),
                    start_date: "01/01/24".to_string(),
                    end_date: "01/15/24".to_string(),
                    percent_complete: 90.0,
                    completed_tickets_count: 100,
                    tickets_added_to_scope_count: 50,
                    open_tickets_added_count: 20,
                },
                CumulativeSprintContext {
                    name: "Sprint 2".to_string(),
                    start_date: "02/01/24".to_string(),
                    end_date: "02/15/24".to_string(),
                    percent_complete: 95.5,
                    completed_tickets_count: 97,
                    tickets_added_to_scope_count: 55,
                    open_tickets_added_count: 25,
                },
            ],
        };
        let blocks = contexts.into_slack_blocks();
        let expected_blocks = vec![
            json!({"type": "section", "text": {"type": "mrkdwn", "text": "\n\n*Previous Sprints:*"}}),
            json!({"type": "section", "text": {"type": "mrkdwn", "text": "01/01/24 - 01/15/24: *100 tickets | 90.00%*"}}),
            json!({"type": "section", "text": {"type": "mrkdwn", "text": "02/01/24 - 02/15/24: *97 tickets | 95.50%*"}}),
        ];
        assert_eq!(blocks, expected_blocks);
    }

    #[test]
    fn test_count_sprints_since_with_existing_name() {
        let contexts = CumulativeSprintContexts {
            history: vec![
                CumulativeSprintContext { name: "Sprint 1".to_string(), ..Default::default() },
                CumulativeSprintContext { name: "Sprint 2".to_string(), ..Default::default() },
            ],
        };
        assert_eq!(contexts.count_sprints_since("Sprint 1"), 2);
    }

    #[test]
    fn test_count_sprints_since_with_non_existing_name() {
        let contexts = CumulativeSprintContexts {
            history: vec![
                CumulativeSprintContext { name: "Sprint 1".to_string(), ..Default::default() },
            ],
        };
        assert_eq!(contexts.count_sprints_since("Sprint 3"), 0);
    }

    #[test]
    fn test_count_sprints_since_empty_history() {
        let contexts = CumulativeSprintContexts { history: vec![] };
        assert_eq!(contexts.count_sprints_since("Sprint 1"), 0);
    }

    #[test]
    fn test_was_sprint_name_used_with_existing_name() {
        let contexts = CumulativeSprintContexts {
            history: vec![
                CumulativeSprintContext { name: "Sprint 1".to_string(), ..Default::default() },
            ],
        };
        assert!(contexts.was_sprint_name_used("Sprint 1"));
    }

    #[test]
    fn test_was_sprint_name_used_with_non_existing_name() {
        let contexts = CumulativeSprintContexts {
            history: vec![
                CumulativeSprintContext { name: "Sprint 1".to_string(), ..Default::default() },
            ],
        };
        assert!(!contexts.was_sprint_name_used("Sprint 2"));
    }

    #[test]
    fn test_was_sprint_name_used_empty_history() {
        let contexts = CumulativeSprintContexts { history: vec![] };
        assert!(!contexts.was_sprint_name_used("Sprint 1"));
    }
}
