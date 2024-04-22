use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::utils::date::{days_between, print_current_date};
use crate::utils::slack_components::{link_element, text_element, user_element};
use super::sprint_records::DailyTicketContext;
use super::ticket_state::TicketState;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckRunDetails {
    pub name: String,
    pub details_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub state: String,
    pub comments: u32,
    pub is_draft: bool,
    pub action_required_check_runs: Vec<CheckRunDetails>,
    pub failing_check_runs: Vec<CheckRunDetails>,
    pub merged: bool,
    pub mergeable: Option<bool>
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TicketDetails {
    pub id: String,
    pub name: String,
    pub state: TicketState,
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
}

impl Ticket {
    //add unit test for emoji/no emoji
    fn ticket_name_new_emoji(&self) -> String {
        if let Ok(days) = days_between(Some(&self.added_on), &print_current_date()) {
            if days <= 2 {
                return "🆕".to_string();
            }
        }

        "".to_string()
    }
    
    //add unit test for emoji/no emoji
    fn ticket_name_age_emoji(&self) -> String {
        if self.sprint_age > 0 {
            return std::iter::repeat("🐌").take(self.sprint_age).collect::<String>();
        }

        "".to_string()
    }
    
    //add unit test for emoji/no emoji
    fn ticket_name_goal_emoji(&self) -> String {
        if self.sprint_age > 0 {
            return "🏁".to_string();
        }

        "".to_string()
    }

    //add unit tests for no name annotation/with name annotation
    fn annotated_ticket_name(&self) -> String {
        let statuses = vec![
            self.ticket_name_new_emoji(), 
            self.ticket_name_age_emoji(), 
            self.ticket_name_goal_emoji()
        ];

        let emoji_string: String = statuses.iter()
            .filter(|status| !status.is_empty())
            .cloned()
            .collect::<Vec<String>>()        
            .join("");         

        if !emoji_string.is_empty() {
            format!("{} {}", emoji_string, self.details.name)
        } else {
            self.details.name.clone()
        }
    }

    //add unit test to assert output format
    fn ticket_name_block(&self) -> Value {
        link_element(&self.details.url, &self.annotated_ticket_name(), Some(json!({"bold": true, "strike": self.details.state == TicketState::BacklogIdeas})))
    }    

    //add unit test for string/no string
    fn check_assignees(&self) -> Option<String> {
        if self.details.state > TicketState::InScope && self.members.is_empty() {
            Some(" | Missing Assignees".to_string())
        } else {
            None
        }
    }

    //add unit test for string/no string
    fn check_description(&self) -> Option<String> {
        if self.details.state > TicketState::InvestigationDiscussion && !self.details.has_description {
            Some(" | Missing Description".to_string())
        } else {
            None
        }
    }

    //add unit test for string/no string
    fn check_labels(&self) -> Option<String> {
        if self.details.state > TicketState::InvestigationDiscussion && !self.details.has_labels {
            Some(" | Missing Labels".to_string())
        } else {
            None
        }
    }

    //add unit test for string/no string
    fn check_pr_missing(&self) -> Option<String> {
        if self.details.state > TicketState::InProgress && self.pr.is_none() {
            Some(" | Missing PR".to_string())
        } else {
            None
        }
    }

    //add unit test for string/no string
    fn check_pr_merged(&self) -> Option<String> {
        if let Some(pr) = &self.pr {
            if self.details.state > TicketState::PendingRelease && !pr.merged {
                Some(" | PR not merged".to_string())
            } else {
                None
            }
        } else {
            None
        }
    }

    //add a unit test for the whole block for no warnings/with warnings
    fn warning_blocks(&self) -> Vec<Value> {
        let mut warnings = Vec::new();

        let checks = vec![
            self.check_description(),
            self.check_labels(),
            self.check_assignees(),
            self.check_pr_missing(),
            self.check_pr_merged(),
        ];

        if checks.iter().any(Option::is_some) {
            warnings.push("\n⚠️".to_string());
        }

        warnings.extend(checks.into_iter().flatten());

        warnings.iter().map(|warning| text_element(warning, Some(json!({"bold": true})))).collect()
    }

    //add a unit test for each branch
    fn pr_link_block(&self, pr: &PullRequest) -> Value {
        link_element(&self.details.pr_url.as_ref().unwrap(),
            if pr.is_draft {
                "🚧 View Draft PR"
            } else {
                "View PR"
            },
            None)
    }

    //add a unit test for each branch
    fn pr_comments_block(&self, pr: &PullRequest) -> Option<Value> {
        if pr.comments > 0 {
            Some(text_element(&format!(" | {} 💬", pr.comments), None))
        } else {
            None
        }
    }

    //add a unit test for each branch
    fn pr_merge_status_block(&self, pr: &PullRequest) -> Value {
        if pr.merged {
            text_element(" | Merged", None)
        } else if pr.mergeable == Some(true) {
            text_element(" | Pending Merge", None)
        } else {
            text_element(" | Can't Merge (see GitHub for details)", Some(json!({"bold": true})))
        }
    }

    //add a unit test for with/without failing check runs
    fn pr_failing_checks_block(&self, pr: &PullRequest) -> Vec<Value> {
        let mut blocks = Vec::new();
        if !pr.failing_check_runs.is_empty() {
            blocks.push(text_element(" | Failing check runs: ", None));
            for check_run in &pr.failing_check_runs {
                blocks.push(link_element(&check_run.details_url, 
                    &check_run.name, 
                    Some(json!({"bold": true, "code": true}))));
            }
        }
        blocks
    }

    //add a unit test for the whole block for no pr data/with pr data
    fn pr_blocks(&self) -> Vec<Value> {
        let mut blocks = vec![];

        if let Some(pr) = &self.pr {
            blocks.push(text_element("\n", None));  // Initial new line for separation
            blocks.push(self.pr_link_block(pr));
            if let Some(comment_block) = self.pr_comments_block(pr) {
                blocks.push(comment_block);
            }
            blocks.push(self.pr_merge_status_block(pr));
            blocks.extend(self.pr_failing_checks_block(pr));
            blocks.push(text_element(" ", None)); // Closing space
        }

        blocks
    }

    //add unit test for with/no checklist items
    fn checklist_blocks(&self) -> Vec<Value> {
        let mut blocks = vec![];
        
        if self.details.checklist_items > 0 {
            blocks.push(text_element("\n", None));
            blocks.push(text_element(&format!("{}/{} completed", self.details.checked_checklist_items, self.details.checklist_items), None));
        }

        blocks
    }

    //add unit test for with multiple members/no members
    fn member_blocks(&self) -> Vec<Value> {
        let mut blocks = vec![];

        if !self.details.member_ids.is_empty() {
            blocks.push(text_element("\n", None));
            for member in &self.members {
                blocks.push(user_element(member));
                blocks.push(text_element(" ", None));
            }
        }

        blocks
    }

    //add unit test with all components to validate the structure
    pub fn into_slack_blocks(&self) -> Value {
        let mut ticket_elements = vec![
            self.ticket_name_block()
        ];
        
        ticket_elements.extend(self.warning_blocks());
        
        ticket_elements.extend(self.pr_blocks());
        
        ticket_elements.extend(self.checklist_blocks());

        ticket_elements.extend(self.member_blocks());

        ticket_elements.push(text_element("\n\n\n", None));

        json!(ticket_elements)
    }
}

impl From<&Ticket> for DailyTicketContext {
    fn from(ticket: &Ticket) -> Self {
        DailyTicketContext {
            id: ticket.details.id.clone(),
            name: ticket.details.name.clone(),
            url: ticket.details.url.clone(),
            state: ticket.details.state.clone(),
            is_goal: ticket.details.is_goal,
            added_on: ticket.added_on.clone(),
            added_in_sprint: ticket.added_in_sprint.clone(),
            last_moved_on: ticket.last_moved_on.clone(),
        }
    }
}

impl From<&DailyTicketContext> for Ticket {
    fn from(record: &DailyTicketContext) -> Self {
        Ticket {
            members: vec![],
            pr: None,
            sprint_age: 0,
            added_in_sprint: record.added_in_sprint.clone(),
            added_on: record.added_on.clone(),
            last_moved_on: record.last_moved_on.clone(),
            details: TicketDetails {            
                id: record.id.clone(),
                name: record.name.clone(),
                state: TicketState::BacklogIdeas,      
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