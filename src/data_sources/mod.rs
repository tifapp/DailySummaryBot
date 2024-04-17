mod github;
mod trello;

use std;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use reqwest::Client;
use trello::{fetch_ticket_details, TicketDetails};
use github::{fetch_pr_details, PullRequest};
use serde_json::{Value, json};
use crate::utils::date::{days_between, print_current_date};
use crate::utils::slack_components::{section_block, divider_block, list_block, link_element, text_element, user_element};
use crate::utils::s3::{create_s3_client, get_sprint_members, get_ticket_data, TicketRecord, TicketRecords};
use aws_sdk_s3;
use aws_config::meta::region::RegionProviderChain;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub added_on: String,
    pub last_moved_on: String,
    pub members: Vec<String>,
    pub details: TicketDetails,
    pub pr: Option<PullRequest>,
}

#[derive(Debug, Serialize)]
pub struct TicketSummary {
    pub blocked_prs: Vec<Ticket>,
    pub open_prs: Vec<Ticket>,
    pub open_tickets: Vec<Ticket>,
    pub completed_tickets: Vec<Ticket>,
    pub goal_tickets: Vec<Ticket>,
    pub backlogged_tickets: Vec<Ticket>,
    pub num_of_tickets: u32,
}

impl TicketSummary {
    pub fn count_open_tickets(&self) -> usize {
        let mut open_ticket_count = 0;

        for _ in &self.open_tickets {
            open_ticket_count += 1
        }
        
        for _ in &self.open_prs {
            open_ticket_count += 1
        }
        
        for _ in &self.blocked_prs {
            open_ticket_count += 1
        }

        open_ticket_count
    }
}

pub async fn fetch_ticket_summary_data() -> Result<TicketSummary> {    
    let fetch_client = Arc::new(Client::new());
    
    let s3_client = create_s3_client().await;
    let user_mapping = Arc::new(get_sprint_members(&s3_client).await?);
    let previous_ticket_data = Arc::new(get_ticket_data(&s3_client).await?);    
    
    let current_ticket_details = fetch_ticket_details( Arc::clone(&fetch_client)).await?;
    let mut current_ticket_ids: Vec<String> = vec![];
    let ticket_features: Vec<_> = current_ticket_details.into_iter().map(|ticket_details| {
        current_ticket_ids.push(ticket_details.id.clone());
        let user_mapping_clone = Arc::clone(&user_mapping);
        let previous_ticket_data_clone = Arc::clone(&previous_ticket_data);
        let fetch_client_clone = Arc::clone(&fetch_client);
        
        async move {     
            let previous_version = previous_ticket_data_clone.as_ref()
                .as_ref()
                .map_or(None, |ticket_records| {
                    ticket_records.tickets.iter()
                        .find(|record| record.id == ticket_details.id)
                });


            Ticket {
                pr:
                    if let Some(url) = &ticket_details.pr_url {
                        let pr_details= fetch_pr_details(fetch_client_clone, url).await.expect("Should get github pr details successfully");
                        Some(pr_details)
                    } else {
                        None
                    },
                added_on: previous_version
                    .map(|record| record.added_on.clone()) 
                    .unwrap_or_else(|| print_current_date()),
                last_moved_on: 
                    if let Some(previous) = &previous_version {
                        if previous.list_name != ticket_details.list_name {
                            print_current_date()
                        } else {
                            previous.last_moved_on.clone()
                        }
                    } else {
                        print_current_date()
                    },
                members: ticket_details.member_ids.iter()
                    .filter_map(|id| {
                        user_mapping_clone.as_ref()
                            .as_ref()
                            .and_then(|map| map.get(id))
                            .map(|name| name.to_string())
                    })
                    .collect(),
                details: ticket_details,
            }
        }
    }).collect();

    let orphaned_tickets: Vec<Ticket> = previous_ticket_data.as_ref() 
        .as_ref()
        .map_or_else(Vec::new, |ticket_records| {
            ticket_records.tickets.iter()
                .filter(|record| !current_ticket_ids.contains(&record.id))
                .map(|record| Ticket::from(record))
                .collect()
        });

    let mut blocked_prs = Vec::new();
    let mut open_prs = Vec::new();
    let mut open_tickets = Vec::new();
    let mut completed_tickets = Vec::new();
    let mut goal_tickets = Vec::new();

    let tickets: Vec<Ticket> = futures::future::join_all(ticket_features).await.into_iter()
        .filter(|ticket| ticket.details.list_name != "Objectives" && ticket.details.list_name != "To Do" && ticket.details.list_name != "Backlog")
        .collect();

    let num_of_tickets = tickets.len();

    for ticket in tickets.into_iter() {
        if ticket.details.list_name == "Done" {
            completed_tickets.push(ticket);
        } else if ticket.details.is_goal {
            goal_tickets.push(ticket);
        } else {
            match &ticket.pr {
                Some(pr) if !pr.failing_check_runs.is_empty() => blocked_prs.push(ticket),
                Some(pr) if !pr.is_draft => open_prs.push(ticket),
                Some(pr) if pr.is_draft => open_tickets.push(ticket),
                Some(_) => open_tickets.push(ticket),
                None => open_tickets.push(ticket),
            }
        }
    }

    Ok(TicketSummary {
        goal_tickets,
        blocked_prs,
        open_prs,
        open_tickets,
        completed_tickets,
        backlogged_tickets: orphaned_tickets,
        num_of_tickets: num_of_tickets as u32,
    })
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

        let mut ticket_elements = vec![link_element(&self.details.url, &ticket_name, Some(json!({"bold": true, "strike": self.details.is_backlogged})))];

        let needs_attention = self.details.list_name != "Investigation/Discussion" && (!self.details.has_description || !self.details.has_labels);
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

impl TicketSummary {
    pub fn into_slack_blocks(&self) -> Vec<Value> {
        let mut blocks: Vec<serde_json::Value> = vec![];

        if !self.goal_tickets.is_empty() {
            blocks.push(divider_block());
            blocks.push(section_block("\n*🏁 Goals*"));
            blocks.push(list_block(self.goal_tickets.iter().map(|ticket| ticket.into_slack_blocks()).collect()));
        }

        if !self.open_prs.is_empty() {
            blocks.push(divider_block());
            blocks.push(section_block("\n*📢 Open PRs*"));
            blocks.push(list_block(self.open_prs.iter().map(|ticket| ticket.into_slack_blocks()).collect()));
        }
        if !self.blocked_prs.is_empty() {
            blocks.push(divider_block());
            blocks.push(section_block("\n*🚨 Blocked PRs*"));
            blocks.push(list_block(self.blocked_prs.iter().map(|ticket| ticket.into_slack_blocks()).collect()));
        }
        if !self.open_tickets.is_empty() {
            blocks.push(divider_block());
            blocks.push(section_block("\n*Open Tickets*"));
            blocks.push(list_block(self.open_tickets.iter().map(|ticket| ticket.into_slack_blocks()).collect()));
        }
        if !self.completed_tickets.is_empty() {
            blocks.push(divider_block());
            blocks.push(section_block("\n*✅ Completed Tickets*"));
            blocks.push(list_block(self.completed_tickets.iter().map(|ticket| ticket.into_slack_blocks()).collect()));
        }
        if !self.backlogged_tickets.is_empty() {
            blocks.push(divider_block());
            blocks.push(section_block("\n*Backlogged Tickets*"));
            blocks.push(list_block(self.backlogged_tickets.iter().map(|ticket| ticket.into_slack_blocks()).collect()));
        }

        blocks.push(divider_block());

        blocks
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
                is_backlogged: true,
                member_ids: vec![],
                pr_url: None,        
            }
        }
    }
}

impl From<TicketSummary> for TicketRecords {
    fn from(summary: TicketSummary) -> Self {
        let mut tickets = Vec::new();

        let mut extend_tickets = |vec: Vec<Ticket>| {
            tickets.extend(vec.iter().map(TicketRecord::from));
        };

        extend_tickets(summary.blocked_prs);
        extend_tickets(summary.open_prs);
        extend_tickets(summary.open_tickets);
        extend_tickets(summary.completed_tickets);
        extend_tickets(summary.goal_tickets);
        extend_tickets(summary.backlogged_tickets);

        TicketRecords { tickets }
    }
}