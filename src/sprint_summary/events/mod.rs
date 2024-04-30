mod slack_events;

use chrono::NaiveDate;
use lambda_runtime::LambdaEvent;
use serde_json::Value;
use crate::utils::http::HttpRequest;
use anyhow::{anyhow, Error, Result};
use super::{sprint_records::{ActiveSprintContext, CumulativeSprintContexts}, SprintCommand, SprintCommandParser};

pub enum SprintEvents {
    MessageTrigger{command: String, args: Vec<String>, channel_id: String, response_url: Option<String>},
    ScheduledTrigger,
}

impl SprintCommandParser for SprintEvents {
    async fn try_into_sprint_command(
        &self, 
        active_sprint_context: &Option<ActiveSprintContext>,
        cumulative_sprint_contexts: &CumulativeSprintContexts,
    ) -> Result<SprintCommand> {
        match active_sprint_context {
            Some(active_sprint_record) => {
                match self {
                    SprintEvents::MessageTrigger { command, args: _, channel_id: _,  response_url: _ } => {
                        match command.as_str() {
                            "/sprint-kickoff" | "/sprint-kickoff-confirm" => {
                                Err(anyhow!("Sprint {} already in progress", active_sprint_record.name))
                            },
                            "/sprint-cancel" => Ok(SprintCommand::SprintCancel),
                            "/sprint-end" => Ok(SprintCommand::SprintEnd),
                            "/sprint-check-in" => Ok(SprintCommand::SprintCheckIn),
                            _ => Err(anyhow!("Invalid command")),
                        }
                    },
                    SprintEvents::ScheduledTrigger => {
                        if active_sprint_record.days_until_end() <= 0 {
                            Ok(SprintCommand::SprintReview)
                        } else {
                            Ok(SprintCommand::DailySummary)
                        }
                    },
                }
            },
            None => {
                match self {
                    SprintEvents::MessageTrigger { command, args, channel_id, response_url: _ } => {
                        match command.as_str() {
                            "/sprint-cancel" | "/sprint-end" | "/sprint-check-in" => {
                                Err(anyhow!("No sprint in progress"))
                            },
                            "/sprint-kickoff" | "/sprint-kickoff-confirm" => {
                                if args.len() < 2 {
                                    return Err(anyhow!("Text field does not contain enough parts"));
                                }
                                
                                NaiveDate::parse_from_str(&args[0], "%m/%d/%Y")
                                    .map_err(|e| format!("Failed to parse date: {}", e));

                                if cumulative_sprint_contexts.was_sprint_name_used(&command) {
                                    Err(anyhow!("Sprint name {} was already used", command))
                                } else if command.as_str() == "/sprint-kickoff-confirm" {
                                    Ok(SprintCommand::SprintKickoff {
                                        end_date: args[0].clone(),
                                        sprint_name: args[1].clone(),
                                        channel_id: channel_id.clone()
                                    })
                                } else {
                                    Ok(SprintCommand::SprintPreview {
                                        end_date: args[0].clone(),
                                        sprint_name: args[1].clone(),
                                        channel_id: channel_id.clone()
                                    })
                                }
                            },
                            _ => Err(anyhow!("Invalid command"))
                        }
                    },
                    SprintEvents::ScheduledTrigger => Err(anyhow!("No sprint in progress")),
                }
            },
        }
    }
}

pub trait MapToSprintEvents {
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

#[cfg(test)]
mod sprint_event_tests {
    use chrono::Local;

    use crate::sprint_summary::sprint_records::CumulativeSprintContext;
    use super::*;

    #[tokio::test]
    async fn test_sprint_preview_with_active_sprint() {
        let active_context = ActiveSprintContext {
            name: "Sprint 1".to_string(),
            start_date: "01/01/22".to_string(),
            end_date: "01/15/22".to_string(),
            channel_id: "C123456".to_string(),
            ..ActiveSprintContext::default()
        };
        let mock_client = Some(active_context);
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::MessageTrigger {
            command: "/sprint-preview".to_string(),
            args: vec!["01/20/22".to_string(), "New Sprint".to_string()],
            channel_id: "C123456".to_string(),
            response_url: None,
        };

        let result = event.try_into_sprint_command(&mock_client, &cumulative_contexts).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sprint_checkin_with_active_sprint() {
        let active_context = ActiveSprintContext {
            name: "Sprint 1".to_string(),
            start_date: "01/01/22".to_string(),
            end_date: "01/15/22".to_string(),
            channel_id: "C123456".to_string(),
            ..ActiveSprintContext::default()
        };
        let mock_client = Some(active_context);
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::MessageTrigger {
            command: "/sprint-check-in".to_string(),
            args: vec![],
            channel_id: "C123456".to_string(),
            response_url: None,
        };

        let result = event.try_into_sprint_command(&mock_client, &cumulative_contexts).await;
        assert!(matches!(result, Ok(SprintCommand::SprintCheckIn)));
    }

    #[tokio::test]
    async fn test_sprint_kickoff_with_used_name() {
        let cumulative_contexts = CumulativeSprintContexts {
            history: vec![CumulativeSprintContext {
                name:"Sprint 1".to_string(), 
                ..CumulativeSprintContext::default()
            }],
        };

        let event = SprintEvents::MessageTrigger {
            command: "/sprint-kickoff-confirm".to_string(),
            args: vec!["02/01/22".to_string(), "Sprint 1".to_string()],
            channel_id: "C789123".to_string(),
            response_url: None,
        };

        let result = event.try_into_sprint_command(&None, &cumulative_contexts).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sprint_kickoff_without_active_sprint_and_no_name_conflict() {
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::MessageTrigger {
            command: "/sprint-kickoff-confirm".to_string(),
            args: vec!["02/01/22".to_string(), "New Sprint".to_string()],
            channel_id: "C789123".to_string(),
            response_url: None,
        };

        let result = event.try_into_sprint_command(&None, &cumulative_contexts).await;
        assert!(matches!(result, Ok(SprintCommand::SprintKickoff { .. })));
    }
    
    #[tokio::test]
    async fn test_daily_summary_with_no_active_sprint() {
        let mock_client = None;
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::ScheduledTrigger;

        let result = event.try_into_sprint_command(&mock_client, &cumulative_contexts).await;
        assert!(result.is_err(), "Daily summary should fail without an active sprint");
    }

    #[tokio::test]
    async fn test_sprint_review_with_active_sprint_due_for_review() {
        let active_context = ActiveSprintContext {
            name: "Sprint 1".to_string(),
            start_date: "01/01/22".to_string(),
            end_date: "01/01/22".to_string(),
            channel_id: "C123456".to_string(),
            ..ActiveSprintContext::default()
        };
        let mock_client = Some(active_context);
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::ScheduledTrigger;

        let result = event.try_into_sprint_command(&mock_client, &cumulative_contexts).await;
        assert!(matches!(result, Ok(SprintCommand::SprintReview)), "Sprint review should be triggered on the last day");
    }

    #[tokio::test]
    async fn test_daily_summary_with_active_sprint_not_due() {
        let active_context = ActiveSprintContext {
            name: "Sprint 1".to_string(),
            start_date: "01/01/22".to_string(),
            end_date: "12/31/22".to_string(),
            channel_id: "C123456".to_string(),
            ..ActiveSprintContext::default()
        };
        let mock_client = Some(active_context);
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::ScheduledTrigger;

        let result = event.try_into_sprint_command(&mock_client, &cumulative_contexts).await;
        assert!(matches!(result, Ok(SprintCommand::DailySummary)), "Daily summary should be generated for active sprints not due for review");
    }

    #[tokio::test]
    async fn test_unrecognized_command() {
        let active_context = Some(ActiveSprintContext {
            name: "Sprint 1".to_string(),
            start_date: "01/01/22".to_string(),
            end_date: "01/15/22".to_string(),
            channel_id: "C123456".to_string(),
            ..ActiveSprintContext::default()
        });
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::MessageTrigger {
            command: "/unknown-command".to_string(),
            args: vec![],
            channel_id: "C123456".to_string(),
            response_url: None,
        };

        let result = event.try_into_sprint_command(&active_context, &cumulative_contexts).await;
        assert!(result.is_err(), "Unrecognized commands should return an error");
    }

    #[tokio::test]
    async fn test_sprint_end_with_no_active_sprint() {
        let mock_client = None;
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::MessageTrigger {
            command: "/sprint-end".to_string(),
            args: vec![],
            channel_id: "C789123".to_string(),
            response_url: None,
        };

        let result = event.try_into_sprint_command(&mock_client, &cumulative_contexts).await;
        assert!(result.is_err(), "Ending a sprint should fail without an active sprint");
    }

    #[tokio::test]
    async fn test_sprint_checkin_without_active_sprint() {
        let mock_client = None; // No active sprint
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::MessageTrigger {
            command: "/sprint-check-in".to_string(),
            args: vec![],
            channel_id: "C789123".to_string(),
            response_url: None,
        };

        let result = event.try_into_sprint_command(&mock_client, &cumulative_contexts).await;
        assert!(result.is_err(), "Check-in should fail without an active sprint");
    }

    #[tokio::test]
    async fn test_daily_summary_with_active_sprint_last_day() {
        let today = Local::today().naive_local();
        let active_context = ActiveSprintContext {
            name: "Sprint 1".to_string(),
            start_date: (today - chrono::Duration::days(14)).format("%m/%d/%Y").to_string(),
            end_date: today.format("%m/%d/%Y").to_string(),
            channel_id: "C123456".to_string(),
            ..ActiveSprintContext::default()
        };
        let mock_client = Some(active_context);
        let cumulative_contexts = CumulativeSprintContexts { history: vec![] };

        let event = SprintEvents::ScheduledTrigger;

        let result = event.try_into_sprint_command(&mock_client, &cumulative_contexts).await;
        assert!(matches!(result, Ok(SprintCommand::SprintReview)), "Should transition to sprint review on the last day");
    }
}
