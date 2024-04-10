use std;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;
use reqwest::Client;
use crate::trello::{fetch_trello_cards, fetch_trello_lists, fetch_board_name};
use crate::github::{fetch_pr_details, PullRequest};
use serde_json::json;
use crate::date::{print_current_date, days_until};
use crate::components::{header_block, section_block, context_block, divider_block, list_block, link_element, text_element, user_element};
use crate::tracing::info;
use serde_json::{Value};
use crate::s3::{get_s3_json};
use aws_sdk_s3;
use aws_config::meta::region::RegionProviderChain;

#[derive(Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub name: String,
    pub list_name: String,
    pub url: String,
    pub pr: Option<PullRequest>,
    pub members: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SprintSummary {
    pub name: String,
    pub blocked_prs: Vec<Ticket>,
    pub open_prs: Vec<Ticket>,
    pub open_tickets: Vec<Ticket>,
    pub completed_tickets: Vec<Ticket>,
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
    let list_name_map: HashMap<String, String> = lists.into_iter().map(|list| (list.id, list.name)).collect();
    
    let region_provider = RegionProviderChain::default_provider().or_else("us-west-2");
    let config = aws_config::from_env().region(region_provider).load().await;
    let s3_client = aws_sdk_s3::Client::new(&config);
    let user_mapping = get_s3_json(s3_client, "trello_to_slack_users.json").await?;

    let board = fetch_board_name(&client, trello_board_id).await?;
    
    let cards = fetch_trello_cards(&client, trello_board_id).await?;

    let ticket_features: Vec<_> = cards.into_iter().filter_map(|card| {
        let list_name = list_name_map.get(&card.idList)?.to_string();
        let member_names: Vec<String> = card.idMembers.iter()
            .filter_map(|id| user_mapping.get(id))
            .cloned()
            .collect();
        if ["Investigation/Discussion", "In Progress", "QA/Bug Testing", "Demo/Final Approval", "Done"].contains(&list_name.as_str()) {
            let pr_url = card.attachments.iter()
                .find_map(|attachment| {
                    if attachment.url.contains("github.com") && attachment.url.contains("/pull/") {
                        Some(attachment.url.clone())
                    } else {
                        None
                    }
                });
                
            let client = Client::new();
            let future = async move {
                let pr = if let Some(url) = &pr_url {
                    let details= fetch_pr_details(&client, url).await.expect("Should get github pr details successfully");
                    Some(details)
                } else {
                    None
                };

                let card_name = if card.desc.as_ref().map_or(true, |d| d.is_empty()) {
                    format!("⚠️ {}", card.name)
                } else {
                    card.name.clone()
                };
    
                Ticket {
                    name: card_name,
                    members: member_names,
                    list_name,
                    url: card.url,
                    pr
                }
            };

            Some(future)
        } else {
            None
        }
    }).collect();

    let mut blocked_prs = Vec::new();
    let mut open_prs = Vec::new();
    let mut open_tickets = Vec::new();
    let mut completed_tickets = Vec::new();

    let tickets = futures::future::join_all(ticket_features).await;

    for ticket in tickets.into_iter() {
        if ticket.list_name == "Done" { //should have a hard enum for trello list names
            completed_tickets.push(ticket);
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

    Ok(SprintSummary {
        name: board,
        blocked_prs,
        open_prs,
        open_tickets,
        completed_tickets,
    })
}

pub fn create_ticket_blocks(tickets: &[Ticket]) -> serde_json::Value {
    let ticket_blocks = tickets.iter().map(|ticket| {
        let mut ticket_elements = vec![link_element(&ticket.url, &ticket.name, Some(json!({"bold": true})))];

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

pub async fn create_daily_summary(slack_channel_id: &str, trello_board_id: &str, end_date: &str) -> Value {
    let board = fetch_sprint_summary_data(trello_board_id).await.expect(&format!("Trello board {} should be accessible", trello_board_id));
    info!("Daily Summary data: {:?}", board);

    let daysUntilEnd = days_until(end_date).expect(&format!("Given date {} should be parseable", end_date));

    let mut blocks: Vec<serde_json::Value> = vec![
        header_block(&format!("🚀 {}: Daily Summary - {}", board.name, print_current_date())), //names should be fun/with a theme, or at least more relevant to our goals
        section_block(&format!("*{} Tickets* Open.\n*{} Days* Remain In Sprint.", board.count_open_tickets(), daysUntilEnd)),
        context_block(&format!("Sprint ends {}.", end_date)),
        divider_block(),
    ];
//remember to add section for goals tickets and pr / checklist status
    if !board.open_prs.is_empty() {
        blocks.push(section_block("\n*📢  Open PRs*"));
        blocks.push(create_ticket_blocks(&board.open_prs));
    }
    if !board.blocked_prs.is_empty() {
        blocks.push(section_block("\n*🚨  Blocked PRs*"));
        blocks.push(create_ticket_blocks(&board.blocked_prs));
    }
    if !board.open_tickets.is_empty() {
        blocks.push(section_block("\n*Open Tickets*"));
        blocks.push(create_ticket_blocks(&board.open_tickets));
    }
    if !board.completed_tickets.is_empty() {
        blocks.push(section_block("\n*✅  Completed Tickets*"));
        blocks.push(create_ticket_blocks(&board.completed_tickets));
    }

    blocks.push(divider_block());

    json!({
        "channel": slack_channel_id,
        "blocks": blocks
    })
}