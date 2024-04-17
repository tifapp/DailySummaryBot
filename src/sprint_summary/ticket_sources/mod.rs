mod github;
mod trello;

use std;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use reqwest::Client;
use trello::{TicketDetails, TicketClient};
use github::{PullRequest, PullRequestClient};
use serde_json::{Value, json};
use crate::utils::date::{days_between, print_current_date};
use crate::utils::slack_components::{section_block, divider_block, list_block, link_element, text_element, user_element};
use crate::utils::s3::{TicketRecord, TicketRecords};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub added_on: String,
    pub last_moved_on: String,
    pub members: Vec<String>,
    pub details: TicketDetails,
    pub pr: Option<PullRequest>,
    pub is_backlogged: bool,
}

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

impl Ticket {
    pub fn into_slack_blocks(&self) -> serde_json::Value {
        let mut ticket_name = self.details.name.clone();
        
        if let Ok(days) = days_between(Some(&self.added_on), &print_current_date()) {
            if days <= 2 {
                ticket_name = format!("🆕 {}", ticket_name);
            }
        }
        
        if let Ok(days) = days_between(Some(&self.last_moved_on), &print_current_date()) {
            if days > 7 {
                ticket_name = format!("🐌 {}", ticket_name);
            }
        }

        let mut ticket_elements = vec![link_element(&self.details.url, &ticket_name, Some(json!({"bold": true, "strike": self.is_backlogged})))];

        //make sure all repos are covered
        let needs_attention = self.details.list_name != "Investigation/Discussion" && (!self.details.has_description || !self.details.has_labels);
        //add warning if in QA but no PR
        if needs_attention {
            ticket_elements.push(text_element("\n", None));
            ticket_elements.push(text_element("⚠️", None));
            if !self.details.has_description {
                ticket_elements.push(text_element(" | Missing Description", Some(json!({"bold": true}))));
            }
            if !self.details.has_labels {
                ticket_elements.push(text_element(" | Missing Labels", Some(json!({"bold": true}))));
            }
        }

        if self.details.checklist_items > 0 {
            ticket_elements.push(text_element("\n", None));
            ticket_elements.push(text_element(&format!("{}/{} completed", self.details.checked_checklist_items, self.details.checklist_items), None));
        }

        if let Some(pr) = &self.pr {
            //Mention the users who are on PR duty that week. Configurable from json file. Make it a weekly thing.
            ticket_elements.push(text_element("\n", None));
            ticket_elements.push(link_element(&pr.pr_url, 
                if pr.is_draft {
                    "🚧 View Draft PR"
                } else {
                    "View PR"
                }, 
                None));

            if pr.comments > 0 {
                ticket_elements.push(text_element(&format!(" | {} 💬", pr.comments), None));
            }

            if !pr.failing_check_runs.is_empty() {
                ticket_elements.push(text_element(" | Failing check runs: ", None));

                for check_run in &pr.failing_check_runs {
                    ticket_elements.push(link_element(&check_run.details_url, 
                        &check_run.name, 
                        Some(json!({"bold": true, "code": true}))));
                }

                ticket_elements.push(text_element(" ", None));
            }  
        }

        if !self.details.member_ids.is_empty() {
            ticket_elements.push(text_element("\n", None));
            for member in &self.members {
                ticket_elements.push(user_element(member));
                ticket_elements.push(text_element(" ", None));
            }
        }

        ticket_elements.push(text_element("\n\n\n", None));

        json!(ticket_elements)
    }
}

impl From<&Ticket> for TicketRecord {
    fn from(ticket: &Ticket) -> Self {
        TicketRecord {
            id: ticket.details.id.clone(),
            name: ticket.details.name.clone(),
            url: ticket.details.url.clone(),
            list_name: ticket.details.list_name.clone(),
            is_goal: ticket.details.is_goal,
            added_on: ticket.added_on.clone(),
            last_moved_on: ticket.last_moved_on.clone(),
        }
    }
}

impl From<&TicketRecord> for Ticket {
    fn from(record: &TicketRecord) -> Self {
        Ticket {
            members: vec![],
            pr: None,
            added_on: record.added_on.clone(),
            last_moved_on: record.last_moved_on.clone(),  
            is_backlogged: true,
            details: TicketDetails {            
                id: record.id.clone(),
                name: record.name.clone(),
                list_name: "None".to_string(),      
                url: record.url.clone(),                          
                has_description: true,   
                has_labels: true,                      
                is_goal: false,  
                checklist_items: 0,
                checked_checklist_items: 0,  
                member_ids: vec![],
                pr_url: None,        
            }
        }
    }
}