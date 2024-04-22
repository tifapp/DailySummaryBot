mod slack_events;

use lambda_runtime::LambdaEvent;
use serde_json::Value;
use crate::utils::{http::HttpRequest, s3::create_json_storage_client};
use anyhow::{anyhow, Error, Result};
use super::{sprint_records::{CumulativeSprintContextClient, CumulativeSprintContexts, LiveSprintContext, LiveSprintContextClient}, SprintContext, SprintEvent, SprintEventParser};

impl From<&LiveSprintContext> for SprintContext {
    fn from(record: &LiveSprintContext) -> Self {
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
    DailySummary,
}

impl SprintEventParser for SprintEvents {
    async fn try_into_sprint_event(self) -> Result<SprintEvent> {
        let json_client = create_json_storage_client().await;

        match json_client.get_sprint_data().await {
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
                    SprintEvents::DailySummary => {
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
                        let history = json_client.get_historical_data().await?.unwrap_or(CumulativeSprintContexts {
                            history: Vec::new(),
                        });

                        if history.was_sprint_name_used(&sprint_event.sprint_context.name) {
                            return Err(anyhow!("Sprint name {} was already used", sprint_event.sprint_context.name));
                        }

                        Ok(sprint_event)
                    },
                    _ => Err(anyhow!("No active sprint data available for this operation")),
                }
            },
            Err(e) => Err(anyhow!("Failed to retrieve sprint data: {}", e)),
        }
    }
}

trait MapToSprintEvents {
    fn try_into_sprint_events(self) -> Result<SprintEvents, Error>;
}

impl MapToSprintEvents for LambdaEvent<Value> {
    fn try_into_sprint_events(self) -> Result<SprintEvents, Error> {
        let request_result: Result<HttpRequest, Error> = self.try_into();

        match request_result {
            Ok(request) => {
                Ok((&request).try_into().expect("should convert into SprintEvents"))
            },
            Err(_) => {
                Ok(SprintEvents::DailySummary)
            }
        }
    }
}

impl SprintEventParser for LambdaEvent<Value> {
    async fn try_into_sprint_event(self) -> Result<SprintEvent> {
        match self.try_into_sprint_events() {
            Ok(sprint_events) => sprint_events.try_into_sprint_event().await,
            Err(e) => Err(e),
        }
    }
}