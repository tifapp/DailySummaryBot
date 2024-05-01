use std::{self, collections::VecDeque};
use std::collections::HashMap;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, Value};

use crate::utils::s3::JsonStorageClient;
use crate::utils::slack_components::section_block;

use super::ticket::TicketLink;
use super::ticket_state::TicketState;

#[async_trait(?Send)]
pub trait SprintMemberClient {
    async fn get_sprint_members(&self) -> Result<Option<HashMap<String, String>>>;
}

#[async_trait(?Send)]
#[cfg(not(test))]
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
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActiveSprintContext {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub channel_id: String,
    pub trello_board: String,
    pub open_tickets_count_beginning: u32,
    pub in_scope_tickets_count_beginning: u32,
}

#[async_trait(?Send)]
pub trait ActiveSprintContextClient {
    async fn get_sprint_data(&self) -> Result<Option<ActiveSprintContext>>;
    async fn put_sprint_data(&self, sprint_record: &ActiveSprintContext) -> Result<()>;
    async fn clear_sprint_data(&self) -> Result<()>;
}

#[async_trait(?Send)]
#[cfg(not(test))]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DailyTicketContext {
    pub id: String,
    pub name: String, 
    pub url: String,
    pub state: TicketState,
    pub is_goal: bool,
    pub added_on: String,
    pub added_in_sprint: String,
    pub last_moved_on: String,
    pub dependency_of: Option<TicketLink>,
}

//Ticket records update daily
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DailyTicketContexts {
    pub tickets: VecDeque<DailyTicketContext>,
}

#[async_trait(?Send)]
pub trait DailyTicketContextClient {
    async fn get_ticket_data(&self) -> Result<Option<DailyTicketContexts>>;
    async fn put_ticket_data(&self, ticket_data: &DailyTicketContexts) -> Result<()>;
}

#[async_trait(?Send)]
#[cfg(not(test))]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CumulativeSprintContext {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub percent_complete: f64,
    pub completed_tickets_count: u32,
    pub tickets_added_to_scope_count: i32,
    pub open_tickets_added_count: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CumulativeSprintContexts {
    pub history: Vec<CumulativeSprintContext>
}

impl CumulativeSprintContexts {
    pub fn into_slack_blocks(&self) -> Vec<Value> {
        if self.history.is_empty() {
            return vec![];
        }
    
        let mut history_text = String::from("\n\n*Previous Sprints:*");
    
        for record in &self.history {
            history_text += &format!(
                "\n{} - {}: *{} tickets | {:.2}%*",
                record.start_date,
                record.end_date,
                record.completed_tickets_count,
                record.percent_complete
            );
        }
    
        vec![section_block(&history_text)]
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
#[async_trait(?Send)]
pub trait CumulativeSprintContextClient {
    async fn get_historical_data(&self) -> Result<Option<CumulativeSprintContexts>>;
    async fn put_historical_data(&self, ticket_data: &CumulativeSprintContexts) -> Result<()>;
}

#[async_trait(?Send)]
#[cfg(not(test))]
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

pub trait SprintClient: SprintMemberClient + CumulativeSprintContextClient + DailyTicketContextClient + ActiveSprintContextClient {}
impl<T> SprintClient for T where T: SprintMemberClient + CumulativeSprintContextClient + DailyTicketContextClient + ActiveSprintContextClient {}

#[cfg(test)]
pub mod mocks {
    use std::collections::{HashMap, VecDeque};
    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use crate::{sprint_summary::ticket_state::TicketState, utils::s3::JsonStorageClient};
    use super::{ActiveSprintContext, ActiveSprintContextClient, CumulativeSprintContext, CumulativeSprintContextClient, CumulativeSprintContexts, DailyTicketContext, DailyTicketContextClient, DailyTicketContexts, SprintClient, SprintMemberClient};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    impl Default for ActiveSprintContext {
        fn default() -> Self {
            ActiveSprintContext {
                start_date: "02/20/23".to_string(),
                end_date: "02/30/23".to_string(),
                name: "Sprint 1".to_string(),
                channel_id: "C123456".to_string(),
                trello_board: "testboard".to_string(),
                open_tickets_count_beginning: 0,
                in_scope_tickets_count_beginning: 0,
            }
        }
    }

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
                    CumulativeSprintContext { name: "Sprint 1".to_string(), ..CumulativeSprintContext::default() },
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
                dependency_of: None,
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
    
    pub struct MockSprintClient {
        sprint_data: Arc<Mutex<Option<ActiveSprintContext>>>,
        historical_data: Arc<Mutex<Option<CumulativeSprintContexts>>>,
        ticket_data: Arc<Mutex<Option<DailyTicketContexts>>>
    }

    impl JsonStorageClient for MockSprintClient {
        async fn get_json(&self, _: &str) -> Result<Option<Value>> {
            Ok(Some(json!("Stub")))
        }

        async fn put_json(&self, _: &str, _: &Value) -> Result<()> {
            Ok(())
        }

        async fn delete_json(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait(?Send)]
    impl ActiveSprintContextClient for MockSprintClient {
        async fn get_sprint_data(&self) -> Result<Option<ActiveSprintContext>, anyhow::Error> {
            let sprint_data = self.sprint_data.lock().await;
            Ok(sprint_data.clone())
        }

        async fn put_sprint_data(&self, sprint_data: &ActiveSprintContext) -> Result<()> {
            let mut sprint_data_lock = self.sprint_data.lock().await;
            *sprint_data_lock = Some(sprint_data.clone());
            Ok(())
        }

        async fn clear_sprint_data(&self) -> Result<()> {
            let mut sprint_data_lock = self.sprint_data.lock().await;
            *sprint_data_lock = None;
            Ok(())
        }
    }

    #[async_trait(?Send)]
    impl DailyTicketContextClient for MockSprintClient {
        async fn get_ticket_data(&self) -> Result<Option<DailyTicketContexts>> {
            let ticket_data = self.ticket_data.lock().await;
            Ok(ticket_data.clone())
        }
    
        async fn put_ticket_data(&self, ticket_data: &DailyTicketContexts) -> Result<()> {
            let mut ticket_data_lock = self.ticket_data.lock().await;
            *ticket_data_lock = Some(ticket_data.clone());
            Ok(())
        }
    }

    #[async_trait(?Send)]
    impl CumulativeSprintContextClient for MockSprintClient {
        async fn get_historical_data(&self) -> Result<Option<CumulativeSprintContexts>> {
            let historical_data = self.historical_data.lock().await;
            Ok(historical_data.clone())
        }

        async fn put_historical_data(&self, historical_data: &CumulativeSprintContexts) -> Result<()> {
            let mut historical_data_lock = self.historical_data.lock().await;
            *historical_data_lock = Some(historical_data.clone());
            Ok(())
        }
    }

    #[async_trait(?Send)]
    impl SprintMemberClient for MockSprintClient {
        async fn get_sprint_members(&self) -> Result<Option<HashMap<String, String>>> {
            let members = HashMap::from([
                ("trello_user1".to_string(), "slack_user1".to_string()),
                ("trello_user2".to_string(), "slack_user2".to_string()),
            ]);
            Ok(Some(members))
        }
    }

    impl MockSprintClient {
        pub fn new(sprint_data: Option<ActiveSprintContext>, historical_data: Option<CumulativeSprintContexts>, ticket_data: Option<DailyTicketContexts>) -> Self {
            Self { 
                sprint_data: Arc::new(Mutex::new(sprint_data)), 
                historical_data: Arc::new(Mutex::new(historical_data)), 
                ticket_data: Arc::new(Mutex::new(ticket_data))
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
            json!(
                {
                    "text": {
                        "text": "\n\n*Previous Sprints:*\n01/01/24 - 01/15/24: *100 tickets | 90.00%*\n02/01/24 - 02/15/24: *97 tickets | 95.50%*",
                        "type": "mrkdwn"
                    },
                    "type": "section"
                }
            ),
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
