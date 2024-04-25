mod slack_events;

use lambda_runtime::LambdaEvent;
use serde_json::Value;
use crate::utils::http::HttpRequest;
use anyhow::{anyhow, Error, Result};
use super::{sprint_records::{ActiveSprintContext, CumulativeSprintContexts, SprintClient}, SprintContext, SprintEvent, SprintEventParser};

impl From<&ActiveSprintContext> for SprintContext {
    fn from(record: &ActiveSprintContext) -> Self {
        SprintContext {
            start_date: record.start_date.clone(),
            end_date: record.end_date.clone(),
            name: record.name.clone(),
            channel_id: record.channel_id.clone(),
        }
    }
}

enum SprintEvents {
    SprintPreview(SprintEvent),
    SprintKickoff(SprintEvent),
    SprintCheckIn,
    ScheduledTrigger,
}

impl SprintEventParser for SprintEvents {
    //unit test all branches
    async fn try_into_sprint_event<'a>(
        &self, 
        sprint_client: &'a dyn SprintClient
    ) -> Result<SprintEvent> {
        match sprint_client.get_sprint_data().await {
            Ok(Some(active_sprint_record)) => {
                match self {
                    SprintEvents::SprintPreview(_) | SprintEvents::SprintKickoff(_) => {
                        Err(anyhow!("Sprint {} already in progress", active_sprint_record.name))
                    },
                    SprintEvents::SprintCheckIn => {
                        Ok(SprintEvent {
                            response_url: None,
                            sprint_command: "/sprint-check-in".to_string(),
                            sprint_context: (&active_sprint_record).into(),
                        })
                    },
                    SprintEvents::ScheduledTrigger => {
                        let sprint_context: SprintContext = (&active_sprint_record).into();
                        
                        Ok(SprintEvent {
                            response_url: None,
                            sprint_command: if sprint_context.days_until_end() <= 0 {
                                "/sprint-review".to_string()
                            } else {
                                "/daily-summary".to_string()
                            },
                            sprint_context: sprint_context,
                        })
                    },
                }
            },
            Ok(None) => {
                match self {
                    SprintEvents::SprintPreview(sprint_event) | SprintEvents::SprintKickoff(sprint_event) => {
                        let history = sprint_client.get_historical_data().await?.unwrap_or(CumulativeSprintContexts {
                            history: Vec::new(),
                        });

                        if history.was_sprint_name_used(&sprint_event.sprint_context.name) {
                            return Err(anyhow!("Sprint name {} was already used", sprint_event.sprint_context.name));
                        }

                        Ok(sprint_event.clone())
                    },
                    _ => Err(anyhow!("No active sprint data available for this operation")),
                }
            },
            Err(e) => Err(anyhow!("Failed to retrieve sprint data: {}", e)),
        }
    }
}

trait MapToSprintEvents {
    fn try_into_sprint_events(&self) -> Result<SprintEvents, Error>;
}

impl MapToSprintEvents for LambdaEvent<Value> {
    fn try_into_sprint_events(&self) -> Result<SprintEvents, Error> {
        let request_result: Result<HttpRequest, Error> = self.try_into();

        match request_result {
            Ok(request) => {
                Ok((&request).try_into().expect("should convert into SprintEvents"))
            },
            Err(_) => {
                Ok(SprintEvents::ScheduledTrigger)
            }
        }
    }
}

impl SprintEventParser for LambdaEvent<Value> {
    async fn try_into_sprint_event<'a>(&self, sprint_client: &'a dyn SprintClient) -> Result<SprintEvent> {
        match self.try_into_sprint_events() {
            Ok(sprint_events) => sprint_events.try_into_sprint_event(sprint_client).await,
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod sprint_event_tests {
    use crate::{sprint_summary::sprint_records::mocks::MockSprintClient, utils::date::print_current_date};
    use super::*;
    use anyhow::anyhow;

    #[tokio::test]
    async fn test_sprint_preview_with_active_sprint() {
        let mock_client = MockSprintClient::new(Some(ActiveSprintContext::default()), None, None);
        let event = SprintEvents::SprintPreview(SprintEvent {
            response_url: None,
            sprint_command: "/sprint-preview".to_string(),
            sprint_context: (&ActiveSprintContext::default()).into(),
        });

        let result = event.try_into_sprint_event(&mock_client).await;
        assert!(result.is_err());
        assert_eq!(format!("{}", result.unwrap_err()), format!("Sprint {} already in progress", "Sprint 1"));
    }

    #[tokio::test]
    async fn test_sprint_checkin_with_active_sprint() {
        let mock_client = MockSprintClient::new(Some(ActiveSprintContext::default()), None, None);
        let event = SprintEvents::SprintCheckIn;

        let result = event.try_into_sprint_event(&mock_client).await.unwrap();
        assert_eq!(result.sprint_command, "/sprint-check-in");
        assert_eq!(result.sprint_context.name, "Sprint 1");
    }

    #[tokio::test]
    async fn test_daily_summary_with_no_active_sprint() {
        let mock_client = MockSprintClient::new(None, None, None);
        let event = SprintEvents::ScheduledTrigger;

        let result = event.try_into_sprint_event(&mock_client).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sprint_kickoff_with_used_name() {
        let mock_client = MockSprintClient::new(None, Some(CumulativeSprintContexts::default()), None);
        let event = SprintEvents::SprintKickoff(SprintEvent {
            response_url: None,
            sprint_command: "/sprint-kickoff".to_string(),
            sprint_context: (&ActiveSprintContext::default()).into(),
        });

        let result = event.try_into_sprint_event(&mock_client).await;
        assert!(result.is_err());
        assert_eq!(format!("{}", result.unwrap_err()), "Sprint name Sprint 1 was already used");
    }

    #[tokio::test]
    async fn test_sprint_kickoff_without_active_sprint_and_no_name_conflict() {
        let mock_client = MockSprintClient::new(None, Some(CumulativeSprintContexts {history: vec![]}), None);
        let event = SprintEvents::SprintKickoff(SprintEvent {
            response_url: None,
            sprint_command: "/sprint-kickoff".to_string(),
            sprint_context: SprintContext {
                start_date: "02/01/22".to_string(),
                end_date: "02/15/22".to_string(),
                name: "New Sprint".to_string(),
                channel_id: "C789123".to_string(),
            },
        });

        let result = event.try_into_sprint_event(&mock_client).await.unwrap();
        assert_eq!(result.sprint_command, "/sprint-kickoff");
        assert_eq!(result.sprint_context.name, "New Sprint");
    }

    #[tokio::test]
    async fn test_daily_summary_with_active_sprint() {
        let mock_client = MockSprintClient::new(Some(ActiveSprintContext::default()), None, None);
        let event = SprintEvents::ScheduledTrigger;

        let result = event.try_into_sprint_event(&mock_client).await;
        assert!(result.is_ok());
        let sprint_event = result.unwrap();
        assert_eq!(sprint_event.sprint_command, "/daily-summary");
        assert_eq!(sprint_event.sprint_context.name, "Sprint 1");
    }

    #[tokio::test]
    async fn test_sprint_checkin_without_active_sprint() {
        let mock_client = MockSprintClient::new(None, None, None);
        let event = SprintEvents::SprintCheckIn;

        let result = event.try_into_sprint_event(&mock_client).await;
        assert!(result.is_err());
        assert_eq!(format!("{}", result.unwrap_err()), "No active sprint data available for this operation");
    }

    #[tokio::test]
    async fn test_daily_summary_with_active_sprint_last_day() {
        let active_context = ActiveSprintContext {
            name: "Sprint 1".to_string(),
            start_date: "01/01/22".to_string(),
            end_date: print_current_date(),
            channel_id: "C123456".to_string(),
            trello_board: "Board123".to_string(),
            open_tickets_count_beginning: 10,
            in_scope_tickets_count_beginning: 5,
        };
        let mock_client = MockSprintClient::new(Some(active_context), None, None);
        let event = SprintEvents::DailySummary;

        let result = event.try_into_sprint_event(&mock_client).await.unwrap();
        assert_eq!(result.sprint_command, "/sprint-review");
        assert_eq!(result.sprint_context.name, "Sprint 1");
    }
}
