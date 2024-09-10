use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::utils::date::{days_between, print_current_date};
use crate::utils::slack_components::{link_element, text_element, user_element};
use super::sprint_records::DailyTicketContext;
use super::ticket_label::TicketLabel;
use super::ticket_state::TicketState;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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

impl PullRequest {
    pub fn is_blocked(&self) -> bool {
        self.merged == false && (self.mergeable != Some(true) || !self.failing_check_runs.is_empty())
    }
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
    pub labels: Vec<TicketLabel>,
    pub checklist_items: u32,
    pub checked_checklist_items: u32,
    pub pr_url: Option<String>,
    pub dependency_of: Option<TicketLink>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TicketLink {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub sprint_age: usize,
    pub is_new: bool,
    pub added_in_sprint: String,
    pub added_on: String,
    pub last_moved_on: String,
    pub moved_out_of_sprint: bool,
    pub members: Vec<String>,
    pub details: TicketDetails,
    pub pr: Option<PullRequest>,
}

impl Ticket {
    pub fn is_goal(&self) -> bool {
        self.details.labels.iter().any(|label| *label == TicketLabel::Goal)
    }    

    fn ticket_name_new_emoji(&self) -> String {
        if self.is_new {
            return "🆕".to_string();
        }

        "".to_string()
    }
    
    fn ticket_name_age_emoji(&self) -> String {
        if self.sprint_age > 0 {
            return std::iter::repeat("🐌").take(self.sprint_age).collect::<String>();
        }

        "".to_string()
    }
    
    fn ticket_name_goal_emoji(&self) -> String {
        if self.is_goal() {
            return "🏁".to_string();
        }

        "".to_string()
    }

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

    fn ticket_name_block(&self) -> Value {
        link_element(&self.details.url, &self.annotated_ticket_name(), Some(json!({"bold": true, "strike": self.moved_out_of_sprint})))
    }    

    fn missing_assignees_warning(&self) -> Option<String> {
        if self.details.state > TicketState::InScope && self.members.is_empty() {
            Some(" | Missing Assignees".to_string())
        } else {
            None
        }
    }

    fn missing_description_warning(&self) -> Option<String> {
        if self.details.state > TicketState::InvestigationDiscussion && !self.details.has_description {
            Some(" | Missing Description".to_string())
        } else {
            None
        }
    }

    fn missing_labels_warning(&self) -> Option<String> {
        if self.details.state > TicketState::InvestigationDiscussion && !self.details.has_labels {
            Some(" | Missing Labels".to_string())
        } else {
            None
        }
    }

    fn missing_pr_warning(&self) -> Option<String> {
        if self.details.state > TicketState::InProgress && self.pr.is_none() {
            Some(" | Missing PR".to_string())
        } else {
            None
        }
    }

    fn unmerged_pr_warning(&self) -> Option<String> {
        if self.details.state > TicketState::PendingRelease {
            match &self.pr {
                Some(pr) if !pr.merged => Some(" | PR not merged".to_string()),
                Some(pr) if pr.merged => None,
                _ => Some(" | PR not merged".to_string())
            }
        } else {
            None
        }
    }

    fn warning_blocks(&self) -> Vec<Value> {
        let mut warnings = Vec::new();

        let checks = vec![
            self.missing_description_warning(),
            self.missing_labels_warning(),
            self.missing_assignees_warning(),
            self.missing_pr_warning(),
            self.unmerged_pr_warning(),
        ];

        if checks.iter().any(Option::is_some) {
            warnings.push("\n⚠️".to_string());
        }

        warnings.extend(checks.into_iter().flatten());

        warnings.iter().map(|warning| text_element(warning, Some(json!({"bold": true})))).collect()
    }

    fn pr_link_block(&self, pr: &PullRequest) -> Value {
        link_element(&self.details.pr_url.as_ref().unwrap(),
            if pr.is_draft {
                "🚧 View Draft PR"
            } else {
                "View PR"
            },
            None)
    }

    fn pr_comments_block(&self, pr: &PullRequest) -> Option<Value> {
        if pr.comments > 0 {
            Some(text_element(&format!(" | {} 💬", pr.comments), None))
        } else {
            None
        }
    }

    fn pr_merge_status_block(&self, pr: &PullRequest) -> Value {
        if pr.merged {
            text_element(" | PR Merged ✔️", None)
        } else if pr.mergeable == Some(true) {
            text_element(" | Pending Merge", None)
        } else {
            text_element(" | Can't Merge (see GitHub for details)", Some(json!({"bold": true})))
        }
    }

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

    fn pr_blocks(&self) -> Vec<Value> {
        let mut blocks = vec![];

        if let Some(pr) = &self.pr {
            blocks.push(text_element("\n", None));
            blocks.push(self.pr_link_block(pr));
            if let Some(comment_block) = self.pr_comments_block(pr) {
                blocks.push(comment_block);
            }
            blocks.push(self.pr_merge_status_block(pr));
            blocks.extend(self.pr_failing_checks_block(pr));
        }

        blocks
    }
    
    fn dependency_blocks(&self) -> Vec<Value> {
        let mut blocks = vec![];
        
        if let Some(dependency) = &self.details.dependency_of {
            blocks.push(text_element("\n", None));
            blocks.push(text_element("Part of ", None));
            blocks.push(link_element(&dependency.url, &dependency.name, None));
        }

        blocks
    }

    fn checklist_blocks(&self) -> Vec<Value> {
        let mut blocks = vec![];
        
        if self.details.checklist_items > 0 {
            blocks.push(text_element("\n", None));
            blocks.push(text_element(&format!("{}/{} completed", self.details.checked_checklist_items, self.details.checklist_items), None));
        }

        blocks
    }

    fn member_blocks(&self) -> Vec<Value> {
        let mut blocks = vec![];

        if !self.members.is_empty() {
            blocks.push(text_element("\n", None));
            for member in &self.members {
                blocks.push(user_element(member));
            }
        }

        blocks
    }

    pub fn into_slack_blocks(&self) -> Value {
        let mut ticket_elements = vec![
            self.ticket_name_block()
        ];
        
        ticket_elements.extend(self.warning_blocks());
        
        ticket_elements.extend(self.pr_blocks());
        
        ticket_elements.extend(self.dependency_blocks());
        
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
            labels: ticket.details.labels.clone(),
            added_on: ticket.added_on.clone(),
            added_in_sprint: ticket.added_in_sprint.clone(),
            last_moved_on: ticket.last_moved_on.clone(),
            dependency_of: ticket.details.dependency_of.clone()
        }
    }
}

impl From<&DailyTicketContext> for Ticket {
    fn from(record: &DailyTicketContext) -> Self {
        Ticket {
            members: vec![],
            pr: None,
            sprint_age: 0,
            moved_out_of_sprint: true,
            added_in_sprint: record.added_in_sprint.clone(),
            added_on: record.added_on.clone(),
            is_new: false,
            last_moved_on: record.last_moved_on.clone(),
            details: TicketDetails {            
                id: record.id.clone(),
                name: record.name.clone(),
                state: TicketState::InScope,      
                url: record.url.clone(),                          
                has_description: true,   
                has_labels: true,                      
                labels: record.labels.clone(),  
                checklist_items: 0,
                checked_checklist_items: 0,  
                member_ids: vec![],
                pr_url: None,      
                dependency_of: record.dependency_of.clone()  
            }
        }
    }
}


#[cfg(test)]
pub mod mocks {
    use crate::sprint_summary::{ticket::TicketDetails, ticket_state::TicketState};

    use super::{PullRequest, Ticket};
    
    impl Default for PullRequest {
        fn default() -> Self {
            PullRequest {
                is_draft: false,
                comments: 3,
                merged: false,
                mergeable: Some(true),
                failing_check_runs: vec![],
                state: "success".to_string(),
                action_required_check_runs: vec![],
            }
        }
    }

    impl Default for TicketDetails {
        fn default() -> Self {
            TicketDetails {
                name: "Mock Task".to_string(),
                url: "http://example.com/mock_ticket".to_string(),
                state: TicketState::InProgress,
                has_description: false,
                has_labels: false,
                checklist_items: 5,
                checked_checklist_items: 3,
                member_ids: vec![],
                id: "abc123".to_string(),
                labels: vec![],
                pr_url: Some("http://github.com/example".to_string()),
                dependency_of: None,
            }
        }
    }
    
    impl Default for Ticket {
        fn default() -> Self {
            Ticket {
                moved_out_of_sprint: false,
                sprint_age: 1,
                added_on: "04/20/24".to_string(),
                is_new: false,
                details: TicketDetails::default(),
                members: vec![],
                pr: Some(PullRequest::default()),
                added_in_sprint: "testsprint".to_string(),
                last_moved_on: "03/20/24".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pr_is_blocked_merged_failing_checks() {
        let pr = PullRequest { 
            failing_check_runs: vec![CheckRunDetails {name: "checkrun1".to_string(), details_url: "examplecheckrun.com".to_string()}], 
            merged: true,
            ..PullRequest::default()
        };
        
        assert_eq!(pr.is_blocked(), false);
    }
    
    #[test]
    fn test_pr_is_blocked_failing_checks() {
        let pr = PullRequest { 
            failing_check_runs: vec![CheckRunDetails {name: "checkrun1".to_string(), details_url: "examplecheckrun.com".to_string()}], ..PullRequest::default() 
        };
        
        assert_eq!(pr.is_blocked(), true);
    }
       
    #[test]
    fn test_pr_is_blocked_mergeable() {
        let pr = PullRequest { 
            mergeable: Some(true),
            merged: false,
            ..PullRequest::default()
        };
        
        assert_eq!(pr.is_blocked(), false);
    }
       
    #[test]
    fn test_pr_is_blocked_not_mergeable() {
        let pr = PullRequest { 
            mergeable: None,
            merged: false,
            ..PullRequest::default()
        };
        
        assert_eq!(pr.is_blocked(), true);
    }

    #[test]
    fn test_pr_is_blocked_merged() {
        let pr = PullRequest { 
            mergeable: Some(false),
            merged: true,
            ..PullRequest::default()
        };
        
        assert_eq!(pr.is_blocked(), false);
    }

    #[test]
    fn test_ticket_name_new_emoji_new() {
        let mut ticket = Ticket::default();
        ticket.is_new = true;
        assert_eq!(ticket.ticket_name_new_emoji(), "🆕");
    }

    #[test]
    fn test_ticket_name_new_emoji_not_new() {
        let mut ticket = Ticket::default();
        ticket.is_new = false;
        assert_eq!(ticket.ticket_name_new_emoji(), "");
    }

    #[test]
    fn test_ticket_name_age_emoji_with_age() {
        let mut ticket = Ticket::default();
        ticket.sprint_age = 3;
        assert_eq!(ticket.ticket_name_age_emoji(), "🐌🐌🐌");
    }

    #[test]
    fn test_ticket_name_age_emoji_without_age() {
        let mut ticket = Ticket::default();
        ticket.sprint_age = 0;
        assert_eq!(ticket.ticket_name_age_emoji(), "");
    }

    #[test]
    fn test_ticket_name_goal_emoji_with_goal() {
        let mut ticket = Ticket::default();
        ticket.details.labels = vec![TicketLabel::Goal];
        assert_eq!(ticket.ticket_name_goal_emoji(), "🏁");
    }

    #[test]
    fn test_ticket_name_goal_emoji_without_goal() {
        let mut ticket = Ticket::default();
        ticket.details.labels = vec![];
        assert_eq!(ticket.ticket_name_goal_emoji(), "");
    }

    #[test]
    fn test_annotated_ticket_name_with_emojis() {
        let mut ticket = Ticket::default();
        ticket.is_new = true;
        ticket.details.labels = vec![TicketLabel::Goal];
        ticket.sprint_age = 2;
        assert_eq!(ticket.annotated_ticket_name(), "🆕🐌🐌🏁 Mock Task");
    }

    #[test]
    fn test_annotated_ticket_name_without_emojis() {
        let mut ticket = Ticket::default();
        ticket.added_on = (chrono::Local::now() - chrono::Duration::try_days(5).unwrap()).format("%m/%d/%y").to_string();
        ticket.details.labels = vec![];
        ticket.sprint_age = 0;
        assert_eq!(ticket.annotated_ticket_name(), "Mock Task");
    }
    
    #[test]
    fn test_ticket_name_block_deferred() {
        let mut ticket = Ticket::default();
        ticket.moved_out_of_sprint = true;
        let expected_blocks = json!({
            "style": {
                "bold": true,
                "strike": true
            },
            "text": "🐌 Mock Task",
            "type": "link",
            "url": "http://example.com/mock_ticket"
        });

        assert_eq!(serde_json::to_value(ticket.ticket_name_block()).unwrap(), expected_blocks);
    }
    
    #[test]
    fn test_ticket_name_block() {
        let ticket = Ticket::default();
        let expected_blocks = json!({
            "style": {
                "bold": true,
                "strike": false
            },
            "text": "🐌 Mock Task",
            "type": "link",
            "url": "http://example.com/mock_ticket"
        });

        assert_eq!(serde_json::to_value(ticket.ticket_name_block()).unwrap(), expected_blocks);
    }

    #[test]
    fn test_missing_assignees_warning_ignore_missing_assignees() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::InScope;
        ticket.members = vec![];
        assert_eq!(ticket.missing_assignees_warning(), None);
    }

    #[test]
    fn test_missing_assignees_warning_without_missing_assignees() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.members.push("User123".to_string());
        assert_eq!(ticket.missing_assignees_warning(), None);
    }
    
    #[test]
    fn test_missing_assignees_warning_with_missing_assignees() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.members = vec![];
        assert_eq!(ticket.missing_assignees_warning(), Some(" | Missing Assignees".to_string()));
    }
    
    #[test]
    fn test_missing_description_warning_ignore_missing_description() {
        //add custom fields for problem, solution, approach, test plan/deliverable? or a trello ticket format
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::InScope;
        ticket.details.has_description = false;
        assert_eq!(ticket.missing_description_warning(), None);
    }

    #[test]
    fn test_missing_description_warning_without_missing_description() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.details.has_description = true;
        assert_eq!(ticket.missing_description_warning(), None);
    }
    
    #[test]
    fn test_missing_description_warning_with_missing_description() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.details.has_description = false;
        assert_eq!(ticket.missing_description_warning(), Some(" | Missing Description".to_string()));
    }

    #[test]
    fn test_missing_labels_warning_ignore_missing_labels() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::InScope;
        ticket.details.has_labels = false;
        assert_eq!(ticket.missing_labels_warning(), None);
    }

    #[test]
    fn test_missing_labels_warning_without_missing_labels() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.details.has_labels = true;
        assert_eq!(ticket.missing_labels_warning(), None);
    }

    #[test]
    fn test_missing_labels_warning_with_missing_labels() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.details.has_labels = false;
        assert_eq!(ticket.missing_labels_warning(), Some(" | Missing Labels".to_string()));
    }

    #[test]
    fn test_missing_pr_warning_ignore() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::InScope;
        ticket.pr = None;
        assert_eq!(ticket.missing_pr_warning(), None);
    }
    
    #[test]
    fn test_missing_pr_warning_without_missing_pr() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.pr = Some(PullRequest::default());
        assert_eq!(ticket.missing_pr_warning(), None);
    }
    
    #[test]
    fn test_missing_pr_warning_with_missing_pr() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.pr = None;
        assert_eq!(ticket.missing_pr_warning(), Some(" | Missing PR".to_string()));
    }
    
    #[test]
    fn test_unmerged_pr_warning_without_pr_ignore() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::InScope;
        ticket.pr = None;
        assert_eq!(ticket.unmerged_pr_warning(), None);
    }
    
    #[test]
    fn test_unmerged_pr_warning_without_pr() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.pr = None;
        assert_eq!(ticket.unmerged_pr_warning(), Some(" | PR not merged".to_string()));
    }
    
    #[test]
    fn test_unmerged_pr_warning_not_merged() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.pr = Some(PullRequest::default());
        ticket.pr.as_mut().unwrap().merged = false;
        assert_eq!(ticket.unmerged_pr_warning(), Some(" | PR not merged".to_string()));
    }   
    
    #[test]
    fn test_unmerged_pr_warning() {
        let mut ticket = Ticket::default();
        ticket.details.state = TicketState::Done;
        ticket.pr = Some(PullRequest::default());
        ticket.pr.as_mut().unwrap().merged = true;
        assert_eq!(ticket.unmerged_pr_warning(), None);
    }  
    
    #[test]
    fn test_warning_blocks_with_warnings() {
        let mut ticket = Ticket::default();
        ticket.pr = Some(PullRequest::default());
        
        let expected_blocks = json!([
            { "style": { "bold": true }, "text": "\n⚠️", "type": "text" },
            { "style": { "bold": true }, "text": " | Missing Description", "type": "text" },
            { "style": { "bold": true }, "text": " | Missing Labels", "type": "text" },
            { "style": { "bold": true }, "text": " | Missing Assignees", "type": "text" }
        ]);

        assert_eq!(serde_json::to_value(ticket.warning_blocks()).unwrap(), expected_blocks);
    }
    
    #[test]
    fn test_warning_blocks_without_warnings() {
        let mut ticket = Ticket::default();
        ticket.details.has_description = true;
        ticket.details.has_labels = true;
        ticket.members = vec!["user1".to_string(), "user2".to_string()];
        ticket.pr = Some(PullRequest::default());

        assert!(ticket.warning_blocks().is_empty());
    }  

    #[test]
    fn test_pr_link_block_draft() {
        let pr = PullRequest {
            is_draft: true,
            ..Default::default()
        };
        let ticket = Ticket::default();
        let expected = json!({
            "style": {},
            "text": "🚧 View Draft PR",
            "type": "link",
            "url": "http://github.com/example"
        });
        assert_eq!(serde_json::to_value(ticket.pr_link_block(&pr)).unwrap(), expected);
    }

    #[test]
    fn test_pr_link_block_not_draft() {
        let pr = PullRequest {
            is_draft: false,
            ..Default::default()
        };
        let ticket = Ticket::default();
        let expected = json!({
            "style": {},
            "text": "View PR",
            "type": "link",
            "url": "http://github.com/example"
        });
        assert_eq!(serde_json::to_value(ticket.pr_link_block(&pr)).unwrap(), expected);
    }

    #[test]
    fn test_pr_comments_block_with_comments() {
        let pr = PullRequest {
            comments: 5,
            ..Default::default()
        };
        let ticket = Ticket::default();
        let expected = json!({
            "style": {},
            "text": " | 5 💬",
            "type": "text"
        });
        assert_eq!(serde_json::to_value(ticket.pr_comments_block(&pr)).unwrap(), expected);
    }

    #[test]
    fn test_pr_comments_block_no_comments() {
        let pr = PullRequest {
            comments: 0,
            ..Default::default()
        };
        let ticket = Ticket::default();
        assert_eq!(ticket.pr_comments_block(&pr), None);
    }

    #[test]
    fn test_pr_merge_status_block_merged() {
        let pr = PullRequest {
            merged: true,
            ..Default::default()
        };
        let ticket = Ticket::default();
        let expected = json!({
            "style": {},
            "text": " | PR Merged ✔️",
            "type": "text"
        });
        assert_eq!(serde_json::to_value(ticket.pr_merge_status_block(&pr)).unwrap(), expected);
    }

    #[test]
    fn test_pr_merge_status_block_pending_merge() {
        let pr = PullRequest {
            merged: false,
            mergeable: Some(true),
            ..Default::default()
        };
        let ticket = Ticket::default();
        let expected = json!({
            "style": {},
            "text": " | Pending Merge",
            "type": "text"
        });
        assert_eq!(serde_json::to_value(ticket.pr_merge_status_block(&pr)).unwrap(), expected);
    }

    #[test]
    fn test_pr_merge_status_block_cannot_merge() {
        let pr = PullRequest {
            merged: false,
            mergeable: Some(false),
            ..Default::default()
        };
        let ticket = Ticket::default();
        let expected = json!({
            "style": {"bold": true},
            "text": " | Can't Merge (see GitHub for details)",
            "type": "text"
        });
        assert_eq!(serde_json::to_value(ticket.pr_merge_status_block(&pr)).unwrap(), expected);
    }

    #[test]
    fn test_pr_failing_checks_block_with_failing_checks() {
        let pr = PullRequest {
            failing_check_runs: vec![
                CheckRunDetails {
                    details_url: "http://example-check.com/2".to_string(),
                    name: "CI Build Failing".to_string(),
                }
            ],
            ..Default::default()
        };
        let ticket = Ticket::default();
        let expected_blocks = json!([
            {
                "style": {},
                "text": " | Failing check runs: ",
                "type": "text"
            },
            {
                "style": {"bold": true, "code": true},
                "text": "CI Build Failing",
                "type": "link",
                "url": "http://example-check.com/2"
            }
        ]);
        assert_eq!(serde_json::to_value(ticket.pr_failing_checks_block(&pr)).unwrap(), expected_blocks);
    }

    #[test]
    fn test_pr_failing_checks_block_no_failing_checks() {
        let pr = PullRequest {
            failing_check_runs: vec![],
            ..Default::default()
        };
        let ticket = Ticket::default();
        assert!(ticket.pr_failing_checks_block(&pr).is_empty());
    }

    #[test]
    fn test_pr_blocks_no_pr() {
        let mut ticket = Ticket::default();
        ticket.pr = None;
        assert!(ticket.pr_blocks().is_empty());
    }
    
    #[test]
    fn test_pr_blocks_with_data() {
        let mut ticket = Ticket::default();
        ticket.pr = Some(PullRequest::default());
        
        let expected_blocks = json!([
            {"type": "text", "text": "\n", "style": {}},
            {"type": "link", "text": "View PR", "url": "http://github.com/example", "style": {}},
            {"type": "text", "text": " | 3 💬", "style": {}},
            {"type": "text", "text": " | Pending Merge", "style": {}},
        ]);

        assert_eq!(serde_json::to_value(ticket.pr_blocks()).unwrap(), expected_blocks);
    }
    
    #[test]
    fn test_dependency_blocks_exist() {
        let mut ticket = Ticket::default();
        ticket.details.dependency_of = Some(TicketLink {
            name: "Greater Objective".to_string(),
            url: "test.com".to_string()
        });

        let expected_blocks = json!([
            {
                "style": {},
                "text": "\n",
                "type": "text"
            },
            {
                "style": {},
                "text": "Part of ",
                "type": "text"
            },
            {
                "style": {},
                "text": "Greater Objective",
                "url": "test.com",
                "type": "link"
            }
        ]);

        assert_eq!(serde_json::to_value(ticket.dependency_blocks()).unwrap(), expected_blocks);
    }

    #[test]
    fn test_dependency_blocks_does_not_exist() {
        let mut ticket = Ticket::default();
        ticket.details.dependency_of = None;
        assert!(ticket.dependency_blocks().is_empty());
    }

    #[test]
    fn test_checklist_blocks_with_items() {
        let mut ticket = Ticket::default();
        ticket.details.checked_checklist_items = 2;
        ticket.details.checklist_items = 3;

        let expected_blocks = json!([
            {
                "style": {},
                "text": "\n",
                "type": "text"
            },
            {
                "style": {},
                "text": "2/3 completed",
                "type": "text"
            }
        ]);

        assert_eq!(serde_json::to_value(ticket.checklist_blocks()).unwrap(), expected_blocks);
    }

    #[test]
    fn test_checklist_blocks_no_items() {
        let mut ticket = Ticket::default();
        ticket.details.checklist_items = 0;
        assert!(ticket.checklist_blocks().is_empty());
    }

    #[test]
    fn test_member_blocks_with_members() {
        let mut ticket = Ticket::default();
        ticket.members = vec!["User123".to_string(), "User456".to_string()];

        let expected_blocks = json!([
            {
                "style": {},
                "text": "\n",
                "type": "text"
            },
            {
                "type": "user",
                "user_id": "User123"
            },
            {
                "type": "user",
                "user_id": "User456"
            }
        ]);

        assert_eq!(serde_json::to_value(ticket.member_blocks()).unwrap(), expected_blocks);
    }

    #[test]
    fn test_member_blocks_no_members() {
        let mut ticket = Ticket::default();
        ticket.members = vec![];
        assert!(ticket.member_blocks().is_empty());
    }

    #[test]
    fn test_into_slack_blocks() {
        let ticket = Ticket::default();
        let expected = json!([
            [ticket.ticket_name_block()],
            ticket.warning_blocks(),     
            ticket.pr_blocks(),          
            ticket.checklist_blocks(),   
            ticket.member_blocks(),      
            [{
                "style": {},
                "text": "\n\n\n",
                "type": "text"
            }]
        ]);

        let merged_blocks = expected.as_array().unwrap().iter()
            .flat_map(|x| x.as_array().unwrap().clone())
            .collect::<Vec<_>>();

        assert_eq!(serde_json::to_value(ticket.into_slack_blocks()).unwrap(), json!(merged_blocks));
    }
}