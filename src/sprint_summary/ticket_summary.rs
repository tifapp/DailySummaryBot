use std::collections::VecDeque;

use serde::Serialize;
use serde_json::Value;
use crate::utils::slack_components::{divider_block, list_block, section_block};
use super::{sprint_records::{DailyTicketContext, DailyTicketContexts}, ticket::Ticket, ticket_state::TicketState};

trait PrioritizedPush {
    fn prioritized_push(&mut self, ticket: Ticket);
}

impl PrioritizedPush for VecDeque<Ticket> {
    fn prioritized_push(&mut self, ticket: Ticket) {
        if ticket.is_goal() {
            self.push_front(ticket);
        } else {
            self.push_back(ticket);
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TicketSummary {
    demoes: VecDeque<Ticket>,
    blocked_prs: VecDeque<Ticket>,
    open_prs: VecDeque<Ticket>,
    open_tickets: VecDeque<Ticket>,
    pub deferred_tickets: VecDeque<Ticket>,
    pub completed_tickets: VecDeque<Ticket>,
    pub sprint_ticket_count: u32,
    pub open_ticket_count: u32,
    project_ticket_count: u32,
    pub project_ticket_count_in_scope: u32,
    pub completed_percentage: f64,
}

impl TicketSummary {
    pub fn clear_completed_and_deferred(&mut self) {
        self.completed_tickets.clear();
        self.deferred_tickets.clear();
    }
}

impl From<Vec<Ticket>> for TicketSummary {
    fn from(tickets: Vec<Ticket>) -> Self {
        let mut demoes = VecDeque::new();
        let mut blocked_prs = VecDeque::new();
        let mut open_prs = VecDeque::new();
        let mut open_tickets = VecDeque::new();
        let mut completed_tickets = VecDeque::new();
        let mut deferred_tickets = VecDeque::new();

        let mut sprint_ticket_count = 0;
        let mut project_ticket_count_in_scope = 0;

        let project_ticket_count = tickets.len() as u32;
        
        for ticket in tickets {
            if ticket.details.state == TicketState::Done {
                sprint_ticket_count += 1;
                completed_tickets.prioritized_push(ticket);
            } else if ticket.details.state <= TicketState::InScope || ticket.moved_out_of_sprint {
                if ticket.details.state == TicketState::InScope {
                    project_ticket_count_in_scope += 1;
                }
                
                if ticket.moved_out_of_sprint {
                    sprint_ticket_count += 1;
                    deferred_tickets.prioritized_push(ticket);
                }
            } else if ticket.details.state == TicketState::DemoFinalApproval {
                sprint_ticket_count += 1;
                demoes.prioritized_push(ticket);
            } else {
                sprint_ticket_count += 1;
                match &ticket.pr {
                    Some(pr) if !pr.is_draft && pr.is_blocked() => {
                        blocked_prs.prioritized_push(ticket);
                    },
                    Some(pr) if !pr.is_draft => {
                        open_prs.prioritized_push(ticket);
                    },
                    Some(_) | None => {
                        open_tickets.prioritized_push(ticket);
                    },
                }
            }
        }

        TicketSummary {
            demoes,
            blocked_prs,
            open_prs,
            open_tickets,
            sprint_ticket_count,
            completed_percentage: (completed_tickets.len() as f64 / sprint_ticket_count as f64) * 100.0,
            project_ticket_count,
            project_ticket_count_in_scope,
            open_ticket_count: sprint_ticket_count - completed_tickets.len() as u32 - deferred_tickets.len() as u32,
            completed_tickets,
            deferred_tickets,
        }
    }
}

impl TicketSummary {
    pub fn into_slack_blocks(&self) -> Vec<Value> {
        let mut blocks: Vec<serde_json::Value> = vec![];

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
        if !self.demoes.is_empty() {
            blocks.push(divider_block());
            blocks.push(section_block("\n*🎥 Demo Available*"));
            blocks.push(list_block(self.demoes.iter().map(|ticket| ticket.into_slack_blocks()).collect()));
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
        if !self.deferred_tickets.is_empty() {
            blocks.push(divider_block());
            blocks.push(section_block("\n*Deferred Tickets*"));
            blocks.push(list_block(self.deferred_tickets.iter().map(|ticket| ticket.into_slack_blocks()).collect()));
        }

        blocks.push(divider_block());

        blocks
    }
}

impl From<&TicketSummary> for DailyTicketContexts {
    fn from(summary: &TicketSummary) -> Self {
        let mut tickets = VecDeque::new();

        let mut extend_tickets = |vec: &VecDeque<Ticket>| {
            tickets.extend(vec.iter().map(|ticket| DailyTicketContext::from(ticket)));
        };

        extend_tickets(&summary.blocked_prs);
        extend_tickets(&summary.open_prs);
        extend_tickets(&summary.open_tickets);
        extend_tickets(&summary.completed_tickets);
        extend_tickets(&summary.deferred_tickets);

        DailyTicketContexts { tickets }
    }
}

#[cfg(test)]
pub mod mocks {
    use std::collections::VecDeque;

    use crate::sprint_summary::ticket::{Ticket, TicketDetails};

    use super::TicketSummary;
    
    impl Default for TicketSummary  {
        fn default() -> Self {
            TicketSummary  {
                completed_tickets: VecDeque::from(vec![
                    Ticket {
                        details: TicketDetails {
                            name: "Completed Ticket".to_string(),
                            ..TicketDetails::default()
                        },
                        ..Ticket::default() 
                    }
                ]),
                demoes: VecDeque::from(vec![
                    Ticket {
                        details: TicketDetails {
                            name: "Ticket To Demo".to_string(),
                            ..TicketDetails::default()
                        },
                        ..Ticket::default() 
                    }
                ]),
                blocked_prs: VecDeque::from(vec![
                    Ticket {
                        details: TicketDetails {
                            name: "Blocked Ticket".to_string(),
                            ..TicketDetails::default()
                        },
                        ..Ticket::default() 
                    }
                ]),
                open_prs: VecDeque::from(vec![
                    Ticket {
                        details: TicketDetails {
                            name: "Needs Review Ticket".to_string(),
                            ..TicketDetails::default()
                        },
                        ..Ticket::default() 
                    }
                ]),
                open_tickets: VecDeque::from(vec![
                    Ticket {
                        details: TicketDetails {
                            name: "Open Ticket".to_string(),
                            ..TicketDetails::default()
                        },
                        ..Ticket::default() 
                    }
                ]),
                deferred_tickets: VecDeque::from(vec![
                    Ticket {
                        details: TicketDetails {
                            name: "Deferred Ticket".to_string(),
                            ..TicketDetails::default()
                        },
                        ..Ticket::default() 
                    }
                ]),
                project_ticket_count: 10,
                open_ticket_count: 20,
                sprint_ticket_count: 15,
                project_ticket_count_in_scope: 80,
                completed_percentage: 0.5,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::sprint_summary::{ticket::{PullRequest, TicketDetails}, ticket_label::TicketLabel};

    #[test]
    fn test_prioritized_push() {
        let mut tickets = VecDeque::new();
        let goal_ticket = Ticket {
            details: TicketDetails { labels: vec![TicketLabel::Goal], ..Default::default() },
            ..Default::default()
        };
        let normal_ticket = Ticket {
            details: TicketDetails { labels: vec![], ..Default::default() },
            ..Default::default()
        };

        tickets.prioritized_push(normal_ticket.clone());
        tickets.prioritized_push(goal_ticket.clone());

        assert_eq!(tickets.front().unwrap().is_goal(), true);
        assert_eq!(tickets.back().unwrap().is_goal(), false);
    }

    #[test]
    fn test_clear_completed_and_deferred() {
        let mut summary = TicketSummary {
            completed_tickets: VecDeque::from(vec![Ticket::default()]),
            deferred_tickets: VecDeque::from(vec![Ticket::default()]),
            ..TicketSummary::default()
        };

        summary.clear_completed_and_deferred();

        assert!(summary.completed_tickets.is_empty());
        assert!(summary.deferred_tickets.is_empty());
    }
    
    #[test]
    fn test_ticket_summary_from_empty_vec() {
        let tickets = vec![];

        let summary = TicketSummary::from(tickets);
        assert_eq!(serde_json::to_value(&summary).expect("summary should be parseable"), json!({
            "demoes": [],
            "blocked_prs": [],
            "open_prs": [],
            "open_tickets": [],
            "deferred_tickets": [],
            "completed_tickets": [],
            "project_ticket_count": 0,
            "sprint_ticket_count": 0,
            "open_ticket_count": 0,
            "project_ticket_count_in_scope": 0,
            "completed_percentage": null
          }));

        let blocks = summary.into_slack_blocks();
        assert_eq!(serde_json::to_value(&blocks).expect("blocks should be parseable"), json!([
            {
              "type": "divider"
            }
          ]
        ));
        
        let contexts: DailyTicketContexts = DailyTicketContexts::from(&summary);
        assert_eq!(serde_json::to_value(&contexts).expect("contexts should be parseable"), json!({"tickets": []}));
    }

    #[test]
    fn test_ticket_summary_from_vec_into_slack_blocks_and_contexts() {
        let completed_ticket = Ticket {
            details: TicketDetails { name: "Completed Ticket".to_string(), state: TicketState::Done, ..TicketDetails::default() },
            ..Ticket::default()
        };
        let demo_ticket = Ticket {
            details: TicketDetails { name: "Ticket To Demo".to_string(), state: TicketState::DemoFinalApproval, ..TicketDetails::default() },
            ..Ticket::default()
        };
        let in_progress_ticket = Ticket {
            details: TicketDetails { name: "In Progress Ticket".to_string(), state: TicketState::InProgress, ..TicketDetails::default() },
            pr: None,
            ..Ticket::default()
        };
        let in_progress_goal_ticket = Ticket {
            details: TicketDetails { name: "In Progress Goal Ticket".to_string(), labels: vec![TicketLabel::Goal], state: TicketState::InProgress, ..TicketDetails::default() },
            pr: None,
            ..Ticket::default()
        };
        let draft_pr_ticket = Ticket {
            details: TicketDetails { name: "Draft PR Open Ticket".to_string(), state: TicketState::InProgress, ..TicketDetails::default() },
            pr: Some(PullRequest { is_draft: true, ..PullRequest::default() }),
            ..Ticket::default()
        };
        let pr_open_ticket = Ticket {
            details: TicketDetails { name: "PR Open Ticket".to_string(), state: TicketState::InProgress, ..TicketDetails::default() },
            pr: Some(PullRequest { is_draft: false, ..PullRequest::default() }),
            ..Ticket::default()
        };
        let pr_blocked_ticket = Ticket {
            details: TicketDetails { name: "PR Blocked Ticket 2".to_string(), state: TicketState::InProgress, ..TicketDetails::default() },
            pr: Some(PullRequest { is_draft: false, mergeable: Some(false), merged: false, ..PullRequest::default() }),
            ..Ticket::default()
        };
        let in_scope_ticket = Ticket {
            details: TicketDetails { name: "In Scope Ticket".to_string(), state: TicketState::InScope, ..TicketDetails::default() },
            ..Ticket::default()
        };
        let in_scope_and_deferred_ticket = Ticket {
            moved_out_of_sprint: true,
            details: TicketDetails { name: "Ticket Moved To In Scope".to_string(), state: TicketState::InScope, ..TicketDetails::default() },
            ..Ticket::default()
        };
        let deferred_ticket = Ticket {
            moved_out_of_sprint: true,
            details: TicketDetails { name: "Deferred Ticket".to_string(), ..TicketDetails::default() },
            ..Ticket::default()
        };
    
        let tickets = vec![
            completed_ticket.clone(),
            demo_ticket.clone(),
            in_progress_ticket.clone(),
            in_progress_goal_ticket.clone(),
            draft_pr_ticket.clone(),
            pr_open_ticket.clone(),
            pr_blocked_ticket.clone(),
            in_scope_and_deferred_ticket.clone(),
            deferred_ticket.clone(),
            in_scope_ticket.clone(),
        ];

        let summary = TicketSummary::from(tickets);
        
        let summary_json = serde_json::to_value(&summary).expect("summary should be parseable");

        assert_eq!(summary_json["project_ticket_count"], 10, "Total number of tickets should be 10");
        assert_eq!(summary_json["project_ticket_count_in_scope"], 2, "Total number of in-scope tickets should be 2");
        assert_eq!(summary_json["sprint_ticket_count"], 9, "Total number of sprint tickets should be 9");
        assert_eq!(summary_json["open_ticket_count"], 6, "Total number of open tickets should be 6");
        assert_eq!(summary_json["completed_percentage"], 100.0*1.0/9.0, "Completed percentage should match #completed/#tickets in sprint scope");
        assert_eq!(summary_json["completed_tickets"], json!(vec![serde_json::to_value(&completed_ticket).unwrap()]), "Completed tickets should match");
        assert_eq!(summary_json["demoes"], json!(vec![serde_json::to_value(&demo_ticket).unwrap()]), "Completed tickets should match");
        assert_eq!(summary_json["open_tickets"], json!(vec![
            serde_json::to_value(&in_progress_goal_ticket).unwrap(), 
            serde_json::to_value(&in_progress_ticket).unwrap(), 
            serde_json::to_value(&draft_pr_ticket).unwrap()
        ]), "Open tickets should match");
        assert_eq!(summary_json["open_prs"], json!(vec![serde_json::to_value(&pr_open_ticket).unwrap()]), "Tickets with open PRs should match");
        assert_eq!(summary_json["blocked_prs"], json!(vec![serde_json::to_value(&pr_blocked_ticket).unwrap()]), "Tickets with blocked PRs should match");
        assert_eq!(summary_json["deferred_tickets"], json!(vec![serde_json::to_value(&in_scope_and_deferred_ticket).unwrap(), serde_json::to_value(&deferred_ticket).unwrap()]), "Deferred tickets should match");
    }
}
