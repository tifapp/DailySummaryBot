use aws_config::meta::region::RegionProviderChain;
use aws_sdk_eventbridge::{types::{RuleState, Target}, Client};
use anyhow::{Result, anyhow};
use crate::tracing::{error, info};

pub async fn create_eventbridge_client() -> Client {
    let region_provider = RegionProviderChain::default_provider().or_else("us-west-2");
    let config = aws_config::from_env().region(region_provider).load().await;
    aws_sdk_eventbridge::Client::new(&config)
}

pub trait EventBridgeExtensions {
    async fn create_daily_trigger_rule(&self, rule_name: &str) -> Result<()>;
    async fn delete_daily_trigger_rule(&self, rule_name: &str) -> Result<()>;
}

impl EventBridgeExtensions for Client {
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
        info!("{:?}", self.describe_rule().name(rule_name).send().await.map_err(|e| anyhow!("Failed to delete rule: {}", e))?);

        self.delete_rule().name(rule_name).send().await.map_err(|e| anyhow!("Failed to delete rule: {}", e))?;

        Ok(())
    }
}

//add a mock that simply saves/reads a value