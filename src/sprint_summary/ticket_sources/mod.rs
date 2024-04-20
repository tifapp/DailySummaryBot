mod github;
mod trello;

use std;
use std::collections::HashMap;
use anyhow::Result;
use reqwest::Client;
use trello::TicketDetailsClient;
use github::PullRequestClient;
use crate::utils::date::print_current_date;
use super::sprint_records::TicketRecords;
use super::ticket::Ticket;
use super::ticket_summary::TicketSummary;

pub trait TicketSummaryClient {
    async fn fetch_ticket_summary(&self, previous_ticket_data: TicketRecords, user_mapping: HashMap<String, String>) -> Result<TicketSummary>;
}

//plug previous ticket data into here
//extract this function out instead of calling it from sprint_summary.rs. That way sprint_summary could be in a separate module?
//extend reqwest client with our custom functions.
impl TicketSummaryClient for Client {
    async fn fetch_ticket_summary(&self, previous_ticket_data: TicketRecords, user_mapping: HashMap<String, String>) -> Result<TicketSummary> {    
        let current_ticket_details = self.fetch_ticket_details().await?;
        let mut current_ticket_ids: Vec<String> = vec![];

        Ok(async {
            let mut result_tickets = Vec::new();
        
            for ticket_details in current_ticket_details {
                current_ticket_ids.push(ticket_details.id.clone());
        
                let previous_version = previous_ticket_data.tickets.iter()
                    .find(|record| record.id == ticket_details.id);
        
                let pr = if let Some(url) = &ticket_details.pr_url {
                    Some(self.fetch_pr_details(url).await.expect("Should get GitHub PR details successfully"))
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
                        .filter_map(|id| user_mapping.get(id)
                            .map(|name| name.to_string()))
                        .collect::<Vec<String>>(),
                    details: ticket_details,
                    is_backlogged: false,
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