mod github;
mod trello;

use std;
use std::collections::HashMap;
use anyhow::{Error, Result};
use async_trait::async_trait;
use crate::utils::date::print_current_date;
use super::sprint_records::{CumulativeSprintContexts, DailyTicketContext, DailyTicketContexts};
use super::ticket::{Ticket, TicketDetails, PullRequest};
use super::ticket_state::TicketState;
use super::ticket_summary::TicketSummary;

pub trait PullRequestClient {
    async fn fetch_pr_details(&self, pr_url: &str) -> Result<PullRequest, Error>;
}

pub trait TicketDetailsClient {
    async fn fetch_ticket_details(&self) -> Result<Vec<TicketDetails>, Error>;
}

struct TicketContext {
    added_on: String,
    added_in_sprint: String,
    sprint_age: usize,
    last_moved_on: String,
}

impl TicketContext {
    fn new_context(ticket_details: &TicketDetails, previous_version: Option<&DailyTicketContext>, current_sprint_name: &str, historical_records: &CumulativeSprintContexts) -> Self {
        if let Some(previous) = previous_version {
            TicketContext {
                added_on: previous.added_on.clone(),
                added_in_sprint: previous.added_in_sprint.clone(),
                sprint_age: historical_records.count_sprints_since(&previous.added_in_sprint),
                last_moved_on: if previous.state != ticket_details.state {
                    print_current_date()
                } else {
                    previous.last_moved_on.clone()
                },
            }
        } else {
            TicketContext {
                added_on: print_current_date(),
                added_in_sprint: current_sprint_name.to_string(),
                sprint_age: 0,
                last_moved_on: print_current_date(),
            }
        }
    }
}


#[async_trait(?Send)]
pub trait TicketSummaryClient {
    async fn fetch_ticket_summary(&self, current_sprint_name: &str, historical_records: &CumulativeSprintContexts, previous_ticket_data: &DailyTicketContexts, user_mapping: HashMap<String, String>) -> Result<TicketSummary>;
}

#[async_trait(?Send)]
impl<T> TicketSummaryClient for T
where
    T: TicketDetailsClient + PullRequestClient + Sync + Send {
    async fn fetch_ticket_summary(&self, current_sprint_name: &str, historical_records: &CumulativeSprintContexts, previous_ticket_data: &DailyTicketContexts, user_mapping: HashMap<String, String>) -> Result<TicketSummary> {    
        let current_ticket_details = self.fetch_ticket_details().await?;
        let mut current_ticket_ids: Vec<String> = vec![];

        Ok(async {
            let mut result_tickets = Vec::new();
        
            for ticket_details in current_ticket_details {
                current_ticket_ids.push(ticket_details.id.clone());
        
                let pr = if let Some(url) = &ticket_details.pr_url {
                    Some(self.fetch_pr_details(url).await.expect("Should get GitHub PR details successfully"))
                } else {
                    None
                };
                
                let previous_version = previous_ticket_data.tickets.iter().find(|record| record.id == ticket_details.id);

                let context = TicketContext::new_context(&ticket_details, previous_version, current_sprint_name, historical_records);
        
                result_tickets.push(Ticket {
                    pr,
                    moved_out_of_sprint: previous_version.is_some() && ticket_details.state <= TicketState::InScope,
                    sprint_age: context.sprint_age,
                    added_on: context.added_on,
                    added_in_sprint: context.added_in_sprint,
                    last_moved_on: context.last_moved_on,
                    members: ticket_details.member_ids.iter()
                        .filter_map(|id| user_mapping.get(id)
                            .map(|name| name.to_string()))
                        .collect::<Vec<String>>(),
                    details: ticket_details,
                });
            }
            
            let orphaned_tickets: Vec<Ticket> = previous_ticket_data.tickets.iter()
                    .filter(|record| !current_ticket_ids.contains(&record.id))
                    .map(|record| Ticket::from(record))
                    .collect();

            result_tickets.extend(orphaned_tickets);
        
            result_tickets.into()
        }.await)
    }
}

#[cfg(test)]
mod ticket_context_tests {
    use super::*;
    use crate::sprint_summary::sprint_records::{CumulativeSprintContexts, DailyTicketContext};
    use crate::sprint_summary::ticket::TicketDetails;
    use crate::sprint_summary::ticket_state::TicketState;
    use crate::utils::date::print_current_date;

    #[test]
    fn context_with_previous_version() {
        let mut ticket_details = TicketDetails::default();
        ticket_details.state = TicketState::Done;
        let ticket_context_default = DailyTicketContext::default();
        let previous_context = Some(&ticket_context_default);
        let historical_records = CumulativeSprintContexts::default();

        let context = TicketContext::new_context(&ticket_details, previous_context, "", &historical_records);

        assert_eq!(context.added_on, "04/01/24");
        assert_eq!(context.last_moved_on, "04/05/24");
        assert_eq!(context.sprint_age, 2);
        assert_eq!(context.added_in_sprint, "Sprint 101");
    }
    
    #[test]
    fn context_with_previous_version_last_moved_updated() {
        let mut ticket_details = TicketDetails::default();
        ticket_details.state = TicketState::InProgress;
        let ticket_context_default = DailyTicketContext::default();
        let previous_context = Some(&ticket_context_default);
        let historical_records = CumulativeSprintContexts::default();

        let context = TicketContext::new_context(&ticket_details, previous_context, "", &historical_records);

        assert_eq!(context.added_on, "04/01/24");
        assert_eq!(context.last_moved_on, print_current_date());
        assert_eq!(context.sprint_age, 2);
        assert_eq!(context.added_in_sprint, "Sprint 101");
    }

    #[test]
    fn context_without_previous_version() {
        let ticket_details = TicketDetails::default();
        let previous_context = None;
        let historical_records = CumulativeSprintContexts::default();
        let current_sprint_name = "Sprint 103";

        let context = TicketContext::new_context(&ticket_details, previous_context, current_sprint_name, &historical_records);

        assert_eq!(context.added_on, print_current_date());
        assert_eq!(context.last_moved_on, print_current_date());
        assert_eq!(context.sprint_age, 0);
        assert_eq!(context.added_in_sprint, "Sprint 103");
    }
}

#[cfg(test)]
pub mod ticket_summary_mocks {
    use std::collections::HashMap;

    use anyhow::{anyhow, Error};
    use crate::sprint_summary::ticket::{PullRequest, TicketDetails};
    use super::{PullRequestClient, TicketDetailsClient};

    pub struct MockPullRequestClient {
        pub responses: HashMap<String, PullRequest>,
    }
    
    impl MockPullRequestClient {
        pub fn new(responses: HashMap<String, PullRequest>) -> Self {
            Self { responses }
        }
    }
    
    impl PullRequestClient for MockPullRequestClient {
        async fn fetch_pr_details(&self, pr_url: &str) -> Result<PullRequest, Error> {
            if let Some(response) = self.responses.get(pr_url) {
                Ok(response.clone())
            } else {
                Err(anyhow!("Pull request not found"))
            }
        }
    }
    
    pub struct MockTicketDetailsClient {
        pub response: Vec<TicketDetails>,
    }
    
    impl MockTicketDetailsClient {
        pub fn new(response: Vec<TicketDetails>) -> Self {
            Self { response }
        }
    }

    impl TicketDetailsClient for MockTicketDetailsClient {
        async fn fetch_ticket_details(&self) -> Result<Vec<TicketDetails>, Error> {
            Ok(self.response.clone())
        }
    }
    
    impl TicketDetailsClient for MockTicketSummaryClient {
        async fn fetch_ticket_details(&self) -> Result<Vec<TicketDetails>, Error> {
            self.ticket_details_client.fetch_ticket_details().await
        }
    }
    
    impl PullRequestClient for MockTicketSummaryClient {
        async fn fetch_pr_details(&self, url: &str) -> Result<PullRequest, Error> {
            self.pull_request_client.fetch_pr_details(url).await
        }
    }
    
    pub struct MockTicketSummaryClient {
        ticket_details_client: MockTicketDetailsClient,
        pull_request_client: MockPullRequestClient,
    }
    
    impl MockTicketSummaryClient {
        pub fn new(ticket_details_client: MockTicketDetailsClient, pull_request_client: MockPullRequestClient) -> Self {
            Self { 
                ticket_details_client,
                pull_request_client
            }
        }
    }
}
#[cfg(test)]
mod ticket_summary_tests {
    use std::collections::{HashMap, VecDeque};
    use serde_json::json;
    use crate::{sprint_summary::{sprint_records::{CumulativeSprintContexts, DailyTicketContext, DailyTicketContexts}, ticket::{PullRequest, Ticket, TicketDetails}, ticket_sources::{ticket_summary_mocks::{MockPullRequestClient, MockTicketDetailsClient, MockTicketSummaryClient}, TicketSummaryClient}, ticket_state::TicketState}, utils::date::print_current_date};
    
    #[tokio::test]
    async fn fetch_summary_combines_data_correctly() {
        let mut pull_request_responses = HashMap::new();
        pull_request_responses.insert("https://default-url.com".to_string(), PullRequest::default());
        pull_request_responses.insert("https://merged-url.com".to_string(), PullRequest { merged: true, ..PullRequest::default() });
        pull_request_responses.insert("https://unmergeable-url.com".to_string(), PullRequest { mergeable: None, merged: false, ..PullRequest::default() });

        let client = MockTicketSummaryClient::new(
            MockTicketDetailsClient::new(vec![
                TicketDetails {
                    name: "Mock Task No PR".to_string(),
                    pr_url: None,
                    id: "mockid".to_string(),
                    state: TicketState::InProgress,
                    ..TicketDetails::default()
                },
                TicketDetails {
                    name: "Mock Task Default".to_string(),
                    pr_url: Some("https://default-url.com".to_string()),
                    ..TicketDetails::default()
                },
                TicketDetails {
                    name: "Mock Task Merged".to_string(),
                    pr_url: Some("https://merged-url.com".to_string()),
                    ..TicketDetails::default()
                },
                TicketDetails {
                    name: "Mock Task Unmergeable".to_string(),
                    pr_url: Some("https://unmergeable-url.com".to_string()),
                    ..TicketDetails::default()
                }
            ]),
            MockPullRequestClient::new(pull_request_responses)
        );
        
        let historical_records = CumulativeSprintContexts::default();
        let previous_ticket_data = DailyTicketContexts {
            tickets: VecDeque::from(vec![
                DailyTicketContext {
                    state: TicketState::InProgress,
                    last_moved_on: "04/15/24".to_string(),
                    id: "mockid".to_string(), 
                    ..DailyTicketContext::default()
                }
            ])
        };
        let user_mapping = HashMap::new();

        let summary = client.fetch_ticket_summary("Current Sprint", &historical_records, &previous_ticket_data, user_mapping).await.unwrap();

        let summary_json = serde_json::to_value(&summary).expect("summary should be parseable");

        assert_eq!(summary_json["open_ticket_count"], 4, "Total number of tickets should be 4");
        assert_eq!(summary_json["open_tickets"], json!(vec![
            serde_json::to_value(&Ticket {
                details: TicketDetails {
                    name: "Mock Task No PR".to_string(),
                    pr_url: None,
                    id: "mockid".to_string(),
                    ..TicketDetails::default()
                },
                pr: None,
                added_in_sprint: "Sprint 101".to_string(), 
                added_on: "04/01/24".to_string(), 
                last_moved_on: "04/15/24".to_string(),
                sprint_age: 2,
                ..Ticket::default()
            }).unwrap(),
        ]), "Fetched open tickets should match");
        assert_eq!(summary_json["open_prs"], json!(vec![
            serde_json::to_value(&Ticket {
                  details: TicketDetails {
                      name: "Mock Task Default".to_string(),
                      pr_url: Some("https://default-url.com".to_string()),
                      ..TicketDetails::default()
                  },
                  pr: Some(PullRequest::default()),
                  added_in_sprint: "Current Sprint".to_string(), 
                  added_on: print_current_date(), 
                  last_moved_on: print_current_date(), 
                  sprint_age: 0,
                  ..Ticket::default()
              }).unwrap(),
              serde_json::to_value(&Ticket {
                  details: TicketDetails {
                      name: "Mock Task Merged".to_string(),
                      pr_url: Some("https://merged-url.com".to_string()),
                      ..TicketDetails::default()
                  },
                  pr: Some(PullRequest { merged: true, ..PullRequest::default() }),
                  added_in_sprint: "Current Sprint".to_string(), 
                  added_on: print_current_date(), 
                  last_moved_on: print_current_date(), 
                  sprint_age: 0,
                  ..Ticket::default()
              }).unwrap(),
        ]), "Fetched open pr tickets should match");
        assert_eq!(summary_json["blocked_prs"], json!(vec![
            serde_json::to_value(&Ticket {
                details: TicketDetails {
                    name: "Mock Task Unmergeable".to_string(),
                    pr_url: Some("https://unmergeable-url.com".to_string()),
                    ..TicketDetails::default()
                },
                pr: Some(PullRequest { mergeable: None, ..PullRequest::default() }),
                added_in_sprint: "Current Sprint".to_string(), 
                added_on: print_current_date(), 
                last_moved_on: print_current_date(), 
                sprint_age: 0,
                ..Ticket::default()
            }).unwrap(),
        ]), "Fetched blocked pr tickets should match");
    }

    #[tokio::test]
    async fn fetch_summary_handles_orphans_correctly() {
        let client = MockTicketSummaryClient::new(
            MockTicketDetailsClient::new(vec![]),
            MockPullRequestClient::new(HashMap::new())
        );
        let historical_records = CumulativeSprintContexts::default();
        
        let previous_ticket_data = DailyTicketContexts { 
            tickets: VecDeque::from(vec![DailyTicketContext {
                id: "orphan123".to_string(),
                ..DailyTicketContext::default()
            }]),
        };
        let user_mapping = HashMap::new();

        let summary = client.fetch_ticket_summary("CurrentSprint", &historical_records, &previous_ticket_data, user_mapping).await.unwrap();

        assert!(summary.deferred_tickets.iter().any(|ticket| ticket.details.id == "orphan123")); //need to make a test-only impl to check that an orphan ticket exists
    }
}
