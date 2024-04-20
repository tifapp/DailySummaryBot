use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::utils::date::{days_between, print_current_date};
use crate::utils::slack_components::{link_element, text_element, user_element};
use super::sprint_records::TicketRecord;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckRunDetails {
    pub name: String,
    pub details_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub pr_url: String,
    pub state: String,
    pub comments: u32,
    pub is_draft: bool,
    pub action_required_check_runs: Vec<CheckRunDetails>,
    pub failing_check_runs: Vec<CheckRunDetails>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TicketDetails {
    pub id: String,
    pub name: String,
    pub list_name: String,
    pub url: String,
    pub member_ids: Vec<String>,
    pub has_description: bool,
    pub has_labels: bool,
    pub is_goal: bool,
    pub checklist_items: u32,
    pub checked_checklist_items: u32,
    pub pr_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub sprint_age: usize,
    pub added_in_sprint: String,
    pub added_on: String,
    pub last_moved_on: String,
    pub members: Vec<String>,
    pub details: TicketDetails,
    pub pr: Option<PullRequest>,
    pub is_backlogged: bool,
}

impl Ticket {
    fn display_sprint_age(&self) -> String {
        std::iter::repeat("🐌").take(self.sprint_age).collect::<String>()
    }

    pub fn into_slack_blocks(&self) -> serde_json::Value {
        let mut ticket_name = self.details.name.clone();
        
        if let Ok(days) = days_between(Some(&self.added_on), &print_current_date()) {
            if days <= 3 {
                ticket_name = format!("🆕 {}", ticket_name);
            }
        }
        
        if self.sprint_age > 0 {
            ticket_name = format!("{} {}", self.display_sprint_age(), ticket_name);
        }

        let mut ticket_elements = vec![link_element(&self.details.url, &ticket_name, Some(json!({"bold": true, "strike": self.is_backlogged})))];

        
        let needs_attention = (self.details.list_name != "Investigation/Discussion" && (!self.details.has_description || !self.details.has_labels))
        || (self.details.list_name == "In Progress" && self.members.is_empty())
        || (self.details.list_name == "QA/Bug Testing" && self.pr.is_none());
        
        if needs_attention {
            ticket_elements.push(text_element("\n", None));
            ticket_elements.push(text_element("⚠️", None));
            if !self.details.has_description {
                ticket_elements.push(text_element(" | Missing Description", Some(json!({"bold": true}))));
            }
            if !self.details.has_labels {
                ticket_elements.push(text_element(" | Missing Labels", Some(json!({"bold": true}))));
            }
            if self.members.is_empty() {
                ticket_elements.push(text_element(" | Missing Assignees", Some(json!({"bold": true}))));
            }
            if self.pr.is_none() {
                ticket_elements.push(text_element(" | Missing PR", Some(json!({"bold": true}))));
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
            added_in_sprint: ticket.added_in_sprint.clone(),
            last_moved_on: ticket.last_moved_on.clone(),
        }
    }
}

impl From<&TicketRecord> for Ticket {
    fn from(record: &TicketRecord) -> Self {
        Ticket {
            members: vec![],
            pr: None,
            sprint_age: 0,
            added_in_sprint: record.added_in_sprint.clone(),
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