use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_eventbridge::{types::{RuleState, Target}, Client};
use anyhow::{Result, anyhow};
use crate::tracing::{error, info};

pub async fn create_eventbridge_client() -> Client {
    let region_provider = RegionProviderChain::default_provider().or_else("us-west-2");
    let config = aws_config::from_env().region(region_provider).load().await;
    aws_sdk_eventbridge::Client::new(&config)
}

#[async_trait(?Send)]
pub trait NotificationClient {
    async fn create_daily_trigger_rule(&self, rule_name: &str) -> Result<()>;
    async fn delete_daily_trigger_rule(&self, rule_name: &str) -> Result<()>;
}

#[async_trait(?Send)]
impl NotificationClient for Client {
    async fn create_daily_trigger_rule(&self, rule_name: &str) -> Result<()> {
        let cron_expression = "cron(0 19 * * ? *)";

        self.put_rule()
            .name(rule_name)
            .schedule_expression(cron_expression)
            .state(RuleState::Enabled)
            .description("Triggers a Lambda daily at 7 PM UTC")
            .send()
            .await
            .map_err(|e| anyhow!("Failed to create rule: {}", e))?;

        let target = Target::builder()
            .arn("arn:aws:lambda:us-west-2:213277979580:function:daily_summary_bot")
            .id("daily_summary_bot")
            .build()?;

        self.put_targets()
            .rule(rule_name)
            .targets(target)
            .send().await.map_err(|e| anyhow!("Failed to set target for rule: {}", e))?;

        Ok(())
    }

    async fn delete_daily_trigger_rule(&self, rule_name: &str) -> Result<()> {
        info!("{:?}", self.describe_rule().name(rule_name).send().await.map_err(|e| anyhow!("Failed to describe rule: {}", e))?);

        self.remove_targets()
            .rule(rule_name)
            .ids("daily_summary_bot")
            .send().await
            .map_err(|e| anyhow!("Failed to remove target: {}", e))?;

        self.delete_rule()
            .name(rule_name)
            .send().await
            .map_err(|e| anyhow!("Failed to delete rule: {}", e))?;

        Ok(())
    }
}


pub mod eventbridge_mocks {
    use std::{collections::HashMap, sync::Arc};
    use tokio::sync::Mutex;
    use super::*;

    pub struct MockEventBridgeClient {
        pub rules_created: Arc<Mutex<HashMap<String, String>>>,
        pub rules_deleted: Arc<Mutex<Vec<String>>>,
    }

    impl MockEventBridgeClient {
        pub fn new() -> Self {
            Self {
                rules_created: Arc::new(Mutex::new(HashMap::new())),
                rules_deleted: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait(?Send)]
    impl NotificationClient for MockEventBridgeClient {
        async fn create_daily_trigger_rule(&self, rule_name: &str) -> Result<()> {
            let cron_expression = "cron(0 19 * * ? *)";
            let mut rules_created = self.rules_created.lock().await;
            rules_created.insert(rule_name.to_string(), cron_expression.to_string());
            Ok(())
        }

        async fn delete_daily_trigger_rule(&self, rule_name: &str) -> Result<()> {
            let mut rules_created = self.rules_created.lock().await;
            if rules_created.contains_key(rule_name) {
                let mut rules_deleted = self.rules_deleted.lock().await;
                rules_deleted.push(rule_name.to_string());
                rules_created.remove(rule_name);
                Ok(())
            } else {
                Err(anyhow!("Rule not found: {}", rule_name))
            }
        }
    }
}