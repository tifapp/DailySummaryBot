use serde::Serialize;
use serde_json::Value;
use super::ticket_sources::Ticket;
use crate::utils::slack_components::{divider_block, list_block, section_block};
use crate::utils::s3::{TicketRecord, TicketRecords};

#[derive(Debug, Serialize)]
pub struct TicketSummary {
    pub blocked_prs: Vec<Ticket>,
    pub open_prs: Vec<Ticket>,
    pub open_tickets: Vec<Ticket>,
    pub completed_tickets: Vec<Ticket>,
    pub goal_tickets: Vec<Ticket>,
    pub backlogged_tickets: Vec<Ticket>,
    pub ticket_count: u32,
    pub open_ticket_count: u32,
    pub completed_percentage: f64,
}

impl From<Vec<Ticket>> for TicketSummary {
    fn from(tickets: Vec<Ticket>) -> Self {
        let mut blocked_prs = Vec::new();
        let mut open_prs = Vec::new();
        let mut open_tickets = Vec::new();
        let mut completed_tickets = Vec::new();
        let mut goal_tickets = Vec::new();
        let mut backlogged_tickets = Vec::new();

        let filtered_tickets: Vec<Ticket> = tickets.into_iter()
            .filter(|ticket| ticket.details.list_name != "Objectives" && ticket.details.list_name != "To Do" && ticket.details.list_name != "Backlog")
            .collect();

        let ticket_count = filtered_tickets.len() as u32;
        let mut open_ticket_count = 0;

        for ticket in filtered_tickets {
            if ticket.is_backlogged {
                backlogged_tickets.push(ticket)
            } else if ticket.details.list_name == "Done" {
                completed_tickets.push(ticket);
            } else if ticket.details.is_goal {
                open_ticket_count += 1;
                goal_tickets.push(ticket);
            } else {
                open_ticket_count += 1;
                match &ticket.pr {
                    Some(pr) if !pr.failing_check_runs.is_empty() => blocked_prs.push(ticket),
                    Some(pr) if !pr.is_draft => open_prs.push(ticket),
                    Some(pr) if pr.is_draft => open_tickets.push(ticket),
                    Some(_) => open_tickets.push(ticket),
                    None => open_tickets.push(ticket),
                }
            }
        }

        TicketSummary {
            completed_percentage: completed_tickets.len() as f64 / ticket_count as f64,
            goal_tickets,
            blocked_prs,
            open_prs,
            open_tickets,
            completed_tickets,
            backlogged_tickets,
            ticket_count,
            open_ticket_count,
        }
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