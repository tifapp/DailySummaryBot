use aws_config::meta::region::RegionProviderChain;
use chrono::NaiveDate;
use anyhow::{Context, Result, anyhow};
use crate::slack::send_message_to_slack;
use crate::ticket_summary::{create_ticket_summary, fetch_ticket_summary_data};
use crate::date::{days_between, print_current_date};
use crate::components::{button_block, header_block, section_block};
use serde_json::Value;
use crate::s3::get_sprint_data;

pub struct SprintInput {
    pub end_date: String,
    pub name: String,
}

fn parse_sprint_input(text: &str) -> Result<SprintInput> {
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return Err(anyhow!("Text field does not contain enough parts"));
    }

    let end_date = parts[0];
    let name = parts[1].to_string();

    NaiveDate::parse_from_str(end_date, "%m/%d/%Y")
        .with_context(|| format!("Failed to parse date: '{}'", end_date))?;

    Ok(SprintInput {end_date: end_date.to_string(), name})
}

pub async fn generate_summary_message(sprint_input: SprintInput, persist_data: bool) -> Result<Vec<Value>> {
    let tickets = fetch_ticket_summary_data(persist_data).await?;

    let daysUntilEnd = days_between(Some(&print_current_date()), &sprint_input.end_date).expect(&format!("Given date {} should be parseable", sprint_input.end_date));
    let completed_ratio = tickets.count_open_tickets() as f64 / tickets.num_of_tickets as f64;

    let mut message_blocks = vec![
        section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", tickets.count_open_tickets(), tickets.num_of_tickets, daysUntilEnd)),
        section_block(&format!("\n*{:.2}% of tasks completed.*", completed_ratio)),
        //add bad, neutral, happy emojis next to percentage
        //add clock if days remaining is too low
        //add some indicator if it looks like we can't finish? could do it in a follow up, we'll see how people use the tool first
    ];

    message_blocks.extend(create_ticket_summary(tickets).await);

    Ok(message_blocks)
}

pub async fn create_sprint_message(command: &str, channel_id: &str, text: &str) -> Result<()> {  
    let region_provider = RegionProviderChain::default_provider().or_else("us-west-2");
    let config = aws_config::from_env().region(region_provider).load().await;
    let s3_client = aws_sdk_s3::Client::new(&config);

    let active_sprint_record = get_sprint_data(&s3_client).await?;
    match command {
        //add extra branch for pushing the button
        //this branch should also start the eventbridge rule
        //this branch should save sprint data to s3
        "/sprint-kickoff" => {
            if active_sprint_record.is_some() {
                Err(anyhow!("A sprint is already in progress"))
            } else {
                let new_sprint_input = parse_sprint_input(&text)?;
                let mut message_blocks = vec![header_block(&format!("🔭 {}: Sprint Preview - {}", new_sprint_input.name, print_current_date()))];
                message_blocks.extend(generate_summary_message(new_sprint_input, false).await?);
                message_blocks.push(button_block("Proceed", "kickoff-sprint", text));
                send_message_to_slack(&channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        //add extra branch for eventbridge trigger - daily summary
        "/sprint-check-in" => {
            if !text.trim().is_empty() {
                Err(anyhow!("No input allowed for sprint check-in"))
            } else if active_sprint_record.is_none() {
                Err(anyhow!("No active sprint"))
            } else {
                let sprint_input = SprintInput::from(&active_sprint_record.expect("should have an active sprint saved"));
                let mut message_blocks = vec![header_block(&format!("🔍 {}: Sprint Check-In - {}", sprint_input.name, print_current_date()))];
                message_blocks.extend(generate_summary_message(sprint_input, true).await?);
                send_message_to_slack(&channel_id, &message_blocks).await.context("Failed to send message to Slack")
            }
        },
        //add extra branch for sprint ending
        //add historical data viewing & pushing
        _ => Err(anyhow!("Unsupported command '{}'", command))
    }
}