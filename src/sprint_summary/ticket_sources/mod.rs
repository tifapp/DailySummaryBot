mod github;
mod trello;

use std;
use std::collections::HashMap;
use anyhow::Result;
use reqwest::Client;
use trello::TicketClient;
use github::PullRequestClient;
use crate::utils::date::print_current_date;
use crate::utils::s3::TicketRecords;
use super::ticket::Ticket;

//plug previous ticket data into here
//extract this function out instead of calling it from sprint_summary.rs. That way sprint_summary could be in a separate module?
//extend reqwest client with our custom functions.
pub async fn fetch_ticket_summary_data(previous_ticket_data: Option<TicketRecords>, user_mapping: Option<HashMap<String, String>>) -> Result<Vec<Ticket>> {    
    let fetch_client = Client::new();    
    let current_ticket_details = fetch_client.fetch_ticket_details().await?;
    let mut current_ticket_ids: Vec<String> = vec![];

    Ok(async {
        let mut result_tickets = Vec::new();
    
        for ticket_details in current_ticket_details {
            current_ticket_ids.push(ticket_details.id.clone());
    
            let previous_version = previous_ticket_data.as_ref()
                .as_ref()
                .map_or(None, |ticket_records| {
                    ticket_records.tickets.iter()
                        .find(|record| record.id == ticket_details.id)
                });
    
            let pr = if let Some(url) = &ticket_details.pr_url {
                Some(fetch_client.fetch_pr_details(url).await.expect("Should get GitHub PR details successfully"))
            } else {
                None
            };
    
            let added_on = previous_version
                .map(|record| record.added_on.clone())
                .unwrap_or_else(|| print_current_date());
    
            let last_moved_on = if let Some(previous) = &previous_version {
                if previous.list_name != ticket_details.list_name {
                    print_current_date()
                } else {
                    previous.last_moved_on.clone()
                }
            } else {
                print_current_date()
            };
    
            result_tickets.push(Ticket {
                pr,
                added_on,
                last_moved_on,
                members: ticket_details.member_ids.iter()
                    .filter_map(|id| user_mapping
                        .as_ref()
                        .and_then(|map| map.get(id))
                        .map(|name| name.to_string()))
                    .collect::<Vec<String>>(),
                details: ticket_details,
                is_backlogged: false,
            });
        }
        
        let orphaned_tickets: Vec<Ticket> = previous_ticket_data
            .as_ref()
            .map_or_else(Vec::new, |ticket_records| {
                ticket_records.tickets.iter()
                    .filter(|record| !current_ticket_ids.contains(&record.id))
                    .map(|record| Ticket::from(record))
                    .collect()
            });

        result_tickets.extend(orphaned_tickets);
    
        result_tickets
    }.await)
}