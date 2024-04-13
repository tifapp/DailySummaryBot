use std;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use reqwest::Client;
use crate::trello::{fetch_ticket_details, TicketDetails};
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

#[derive(Debug, Serialize)]
pub struct SprintSummary {
    pub name: String,
    pub end_date: String,
    pub tickets: TicketSummary,
}

impl TicketSummary {
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

//maybe for a high priority item, we need to have all hands on it
//should help chetan allocate resources, figure out who should be working on what, and whether rescoping is necessary and/or whether our goals are clear, feasible and attainable
pub async fn fetch_ticket_summary_data(trello_board_id: &str) -> Result<TicketSummary> {    
    let fetch_client = Arc::new(Client::new());
    
    let region_provider = RegionProviderChain::default_provider().or_else("us-west-2");
    let config = aws_config::from_env().region(region_provider).load().await;
    let s3_client = aws_sdk_s3::Client::new(&config);
    let user_mapping = Arc::new(get_sprint_members(&s3_client).await?);
    let previous_ticket_data = Arc::new(get_ticket_data(&s3_client).await?);    
    
    let current_ticket_details = fetch_ticket_details( Arc::clone(&fetch_client), trello_board_id).await?;
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

    let mut ticket_records = TicketRecords { tickets: vec![] };

    for ticket in tickets.into_iter() {
        ticket_records.tickets.push(TicketRecord::from(&ticket));
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
    
    put_ticket_data(&s3_client, &ticket_records).await?;

    Ok(TicketSummary {
        goal_tickets,
        blocked_prs,
        open_prs,
        open_tickets,
        completed_tickets,
        backlogged_tickets: orphaned_tickets,
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
    //give some kind of ranking? F-S+. Actually we should allow for a 102+% completion ranking if you complete more tickets than initally in the sprint. well no, then people would be encouraged to make really small sprints and add additional tickets later.
    
    //dont ping people during the weekend. or ask them if they want to
    let ticket_blocks = tickets.iter().map(|ticket| {
        let mut ticket_name = ticket.details.name.clone();

        let mut ticket_elements = vec![link_element(&ticket.details.url, &ticket_name, Some(json!({"bold": true, "strike": ticket.details.is_backlogged})))];
        
        let needs_attention = ticket.details.list_name != "Investigation/Discussion" && (!ticket.details.has_description || !ticket.details.has_labels);        
        if needs_attention {
            ticket_elements.push(text_element("\n", None));
            ticket_elements.push(text_element("⚠️", None));
            if !ticket.details.has_description {
                ticket_elements.push(text_element(" | Missing Description", None));
            }
            if !ticket.details.has_labels {
                ticket_elements.push(text_element(" | Missing Labels", None));
            }
        }
        
        if (ticket.details.checklist_items > 0) {
            ticket_elements.push(text_element("\n", None));
            ticket_elements.push(text_element(&format!("{}/{} completed", ticket.details.checked_checklist_items, ticket.details.checklist_items), None));
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
        
        if (!ticket.details.member_ids.is_empty()) {
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
    let completed_ratio = sprint_summary.tickets.count_open_tickets() as f64 / sprint_summary.tickets.num_of_tickets as f64;
    let mut blocks: Vec<serde_json::Value> = vec![
        header_block(&format!("🚀 {}: Daily Summary - {}", sprint_summary.name, print_current_date())), //names should be fun/with a theme, or at least more relevant to our goals
        section_block(&format!("*{}/{} Tickets* Open.\n*{} Days* Remain In Sprint.", sprint_summary.tickets.count_open_tickets(), sprint_summary.tickets.num_of_tickets, daysUntilEnd)),
        section_block(&format!("\n*{:.2}% of tasks completed.*", completed_ratio)),
        //add bad, neutral, happy emojis next to percentage
        //add clock if days remaining is too low
        //add some indicator if it looks like we can't finish? could do it in a follow up, we'll see how people use the tool first
    ];

    if !sprint_summary.tickets.goal_tickets.is_empty() {
        blocks.push(divider_block());
        blocks.push(section_block("\n*🏁 Goals*"));
        blocks.push(create_ticket_blocks(&sprint_summary.tickets.goal_tickets));
    };

    blocks.push(divider_block());

    if !sprint_summary.tickets.open_prs.is_empty() {
        blocks.push(section_block("\n*📢  Open PRs*"));
        blocks.push(create_ticket_blocks(&sprint_summary.tickets.open_prs));
    }
    if !sprint_summary.tickets.blocked_prs.is_empty() {
        blocks.push(section_block("\n*🚨  Blocked PRs*"));
        blocks.push(create_ticket_blocks(&sprint_summary.tickets.blocked_prs));
    }
    if !sprint_summary.tickets.open_tickets.is_empty() {
        blocks.push(section_block("\n*Open Tickets*"));
        blocks.push(create_ticket_blocks(&sprint_summary.tickets.open_tickets));
    }
    if !sprint_summary.tickets.completed_tickets.is_empty() {
        blocks.push(section_block("\n*✅  Completed Tickets*"));
        blocks.push(create_ticket_blocks(&sprint_summary.tickets.completed_tickets));
    }
    if !sprint_summary.tickets.backlogged_tickets.is_empty() {
        blocks.push(section_block("\n*Backlogged Tickets*"));
        blocks.push(create_ticket_blocks(&sprint_summary.tickets.backlogged_tickets));
    }

    blocks.push(divider_block());

    blocks
}