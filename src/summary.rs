use std;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;
use reqwest::Client;
use crate::trello::{fetch_trello_cards, fetch_trello_lists};
use crate::github::{fetch_pr_details, PullRequest};
use serde_json::json;
use crate::date::{print_current_date, days_until};
use crate::components::{header_block, section_block, context_block, divider_block, list_block, link_element, text_element, user_element};
use serde_json::{Value};
use crate::s3::{get_sprint_data, get_sprint_members, get_ticket_data, put_ticket_data, TicketRecord, TicketRecords};
use aws_sdk_s3;
use aws_config::meta::region::RegionProviderChain;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub id: String,
    pub name: String,
    pub list_name: String,
    pub url: String,
    pub pr: Option<PullRequest>,
    pub members: Vec<String>,
    pub is_new: bool,
    pub has_description: bool,
    pub has_labels: bool,
    pub is_goal: bool,
    pub is_backlogged: bool,
    pub checklist_items: u32,
    pub checked_checklist_items: u32,
}

#[derive(Debug, Serialize)]
pub struct SprintSummary {
    pub name: String,
    pub end_date: String,
    pub blocked_prs: Vec<Ticket>,
    pub open_prs: Vec<Ticket>,
    pub open_tickets: Vec<Ticket>,
    pub completed_tickets: Vec<Ticket>,
    pub goal_tickets: Vec<Ticket>,
    pub backlogged_tickets: Vec<Ticket>,
    pub num_of_tickets: u32,
}

impl SprintSummary {
    pub fn count_open_tickets(&self) -> usize {
        let mut open_ticket_count = 0;

        for ticket in &self.open_tickets {
            open_ticket_count += 1
        }
        
        for ticket in &self.open_prs {
            open_ticket_count += 1
        }
        
        for ticket in &self.blocked_prs {
            open_ticket_count += 1
        }

        open_ticket_count
    }
}

pub async fn fetch_sprint_summary_data(trello_board_id: &str) -> Result<SprintSummary> {    
    let client = Client::new();

    let lists = fetch_trello_lists(&client, trello_board_id).await?;
    let list_name_map = Arc::new(lists.into_iter().map(|list| (list.id, list.name)).collect::<HashMap<_, _>>());
    
    let region_provider = RegionProviderChain::default_provider().or_else("us-west-2");
    let config = aws_config::from_env().region(region_provider).load().await;
    let s3_client = aws_sdk_s3::Client::new(&config);
    let user_mapping = Arc::new(get_sprint_members(&s3_client).await?);
    
    let cards = fetch_trello_cards(&client, trello_board_id).await?;
    
    let sprint_data = get_sprint_data(&s3_client).await?;
    let previous_ticket_data = Arc::new(get_ticket_data(&s3_client).await?);

    let mut current_ticket_ids: Vec<String> = vec![];
    let ticket_features: Vec<_> = cards.into_iter().map(|card| {
        current_ticket_ids.push(card.id.clone());
        let user_mapping_clone = Arc::clone(&user_mapping);
        let list_name_map_clone = Arc::clone(&list_name_map);
        let previous_ticket_data_clone = Arc::clone(&previous_ticket_data);
        let pr_url = card.attachments.iter()
            .find_map(|attachment| {
                if attachment.url.contains("github.com") && attachment.url.contains("/pull/") {
                    Some(attachment.url.clone())
                } else {
                    None
                }
            });
            
        let client = Client::new();
        
        async move {
            let pr = if let Some(url) = &pr_url {
                let details= fetch_pr_details(&client, url).await.expect("Should get github pr details successfully");
                Some(details)
            } else {
                None
            };

            let is_new = !previous_ticket_data_clone.tickets.iter().any(|ticket| ticket.id == card.id);

            Ticket {
                id: card.id.clone(),
                name: card.name,
                members: card.idMembers.iter()
                    .filter_map(|id| user_mapping_clone.get(id))
                    .cloned()
                    .collect(),
                list_name: list_name_map_clone.get(&card.idList).unwrap_or(&"Default List Name".to_string()).clone(),
                url: card.url,
                pr,
                has_labels:
                    if card.labels.len() > 0 {
                        true
                    } else {
                        false
                    },
                has_description: 
                    if card.desc.as_ref().map_or(true, |d| d.is_empty()) {
                        false
                    } else {
                        true
                    },
                is_goal: card.labels.iter().any(|label| label.name == "Goal"),
                is_new,
                checklist_items: card.badges.checkItems,
                checked_checklist_items: card.badges.checkItemsChecked,
                is_backlogged: false,
            }
        }
    }).collect();

    let unmatched_tickets: Vec<Ticket> = previous_ticket_data.tickets.iter()
        .filter(|record| !current_ticket_ids.contains(&record.id))
        .collect::<Vec<&TicketRecord>>()
        .iter()
        .map(|record| Ticket {
            id: record.id.clone(),
            name: record.name.clone(),
            list_name: "None".to_string(),      
            url: record.url.clone(),
            pr: None,                                       
            members: vec![],                                
            is_new: false,                                   
            has_description: true,   
            has_labels: true,                      
            is_goal: false,  
            checklist_items: 0,
            checked_checklist_items: 0,    
            is_backlogged: true,                
        })
        .collect();

    let mut blocked_prs = Vec::new();
    let mut open_prs = Vec::new();
    let mut open_tickets = Vec::new();
    let mut completed_tickets = Vec::new();
    let mut goal_tickets = Vec::new();

    let tickets: Vec<Ticket> = futures::future::join_all(ticket_features).await.into_iter()
        .filter(|ticket| ticket.list_name != "Objectives" && ticket.list_name != "To Do" && ticket.list_name != "Backlog")
        .collect();

    let mut ticket_records = TicketRecords { tickets: vec![] };

    for ticket in tickets.into_iter() {
        ticket_records.tickets.push(TicketRecord::from(ticket.clone()));
        if ticket.list_name == "Done" {
            completed_tickets.push(ticket);
        } else if ticket.is_goal {
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
    
    put_ticket_data(&s3_client, &ticket_records).await?;

    Ok(SprintSummary {
        name: sprint_data.name,
        end_date: sprint_data.end_date,
        goal_tickets: goal_tickets,
        blocked_prs,
        open_prs,
        open_tickets,
        completed_tickets,
        backlogged_tickets: unmatched_tickets,
        num_of_tickets: ticket_records.tickets.len() as u32,
    })
}

//should probably just take a singular ticket, and we'll move the map outside this function
// makes it easier to add tests
pub fn create_ticket_blocks(tickets: &[Ticket]) -> serde_json::Value {
    // add emoji indicators for these:
    // pub is_new: bool,
    //compare list of old tickets with new tickets. for new tickets, add new emoji next to it.
    //goals tickets should have an emoji or color that shows what state the goal ticket is in. maybe red, yellow, green? or something more descriptive like no emoji, in progress, checkmark, etc
    let ticket_blocks = tickets.iter().map(|ticket| {
        let mut ticket_name = ticket.name.clone();

        let mut ticket_elements = vec![link_element(&ticket.url, &ticket_name, Some(json!({"bold": true, "strike": ticket.is_backlogged})))];
        
        let needs_attention = ticket.list_name != "Investigation/Discussion" && (!ticket.has_description || !ticket.has_labels);        
        if needs_attention {
            ticket_elements.push(text_element("\n", None));
            ticket_elements.push(text_element("⚠️", None));
            if !ticket.has_description {
                ticket_elements.push(text_element(" | Missing Description", None));
            }
            if !ticket.has_labels {
                ticket_elements.push(text_element(" | Missing Labels", None));
            }
        }
        
        if (ticket.checklist_items > 0) {
            ticket_elements.push(text_element("\n", None));
            ticket_elements.push(text_element(&format!("{}/{} completed", ticket.checked_checklist_items, ticket.checklist_items), None));
        }

        if let Some(pr) = &ticket.pr {
            ticket_elements.push(text_element("\n", None));

            ticket_elements.push(link_element(&pr.pr_url, 
            if (pr.is_draft) {
                "🚧 View Draft PR"
            } else {
                "View PR"
            }, 
            None));
            
            if (pr.comments > 0) {
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
        
        if (!ticket.members.is_empty()) {
            ticket_elements.push(text_element("\n", None));

            for member in &ticket.members {
                ticket_elements.push(user_element(&member));
                ticket_elements.push(text_element(" ", None));
            }
        }

        ticket_elements.push(text_element("\n\n\n", None));

        json!(ticket_elements)
    }).collect::<Vec<_>>();

    list_block(ticket_blocks)
}


pub async fn format_summary_message(sprint_summary: SprintSummary) -> Vec<Value> {
    let daysUntilEnd = days_until(&sprint_summary.end_date).expect(&format!("Given date {} should be parseable", sprint_summary.end_date));
    let completed_ratio = sprint_summary.count_open_tickets() as f64 / sprint_summary.num_of_tickets as f64;
    let mut blocks: Vec<serde_json::Value> = vec![
        header_block(&format!("🚀 {}: Daily Summary - {}", sprint_summary.name, print_current_date())), //names should be fun/with a theme, or at least more relevant to our goals
        section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", sprint_summary.count_open_tickets(), sprint_summary.num_of_tickets, daysUntilEnd)),
        section_block(&format!("\n*{:.2}% of tasks completed.*", completed_ratio)),
    ];

    if !sprint_summary.goal_tickets.is_empty() {
        blocks.push(divider_block());
        blocks.push(section_block("\n*🏁 Goals*"));
        blocks.push(create_ticket_blocks(&sprint_summary.goal_tickets));
    };

    blocks.push(divider_block());

    if !sprint_summary.open_prs.is_empty() {
        blocks.push(section_block("\n*📢  Open PRs*"));
        blocks.push(create_ticket_blocks(&sprint_summary.open_prs));
    }
    if !sprint_summary.blocked_prs.is_empty() {
        blocks.push(section_block("\n*🚨  Blocked PRs*"));
        blocks.push(create_ticket_blocks(&sprint_summary.blocked_prs));
    }
    if !sprint_summary.open_tickets.is_empty() {
        blocks.push(section_block("\n*Open Tickets*"));
        blocks.push(create_ticket_blocks(&sprint_summary.open_tickets));
    }
    if !sprint_summary.completed_tickets.is_empty() {
        blocks.push(section_block("\n*✅  Completed Tickets*"));
        blocks.push(create_ticket_blocks(&sprint_summary.completed_tickets));
    }
    if !sprint_summary.backlogged_tickets.is_empty() {
        blocks.push(section_block("\n*Backlogged Tickets*"));
        blocks.push(create_ticket_blocks(&sprint_summary.backlogged_tickets));
    }

    blocks.push(divider_block());

    blocks
}