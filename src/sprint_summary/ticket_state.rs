use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Deserialize, Serialize)]
pub enum TicketState {
    BacklogIdeas,
    InScope,
    InvestigationDiscussion,
    InProgress,
    PendingRelease,
    DemoFinalApproval,
    Done,
}

impl TicketState {
    pub fn from_str(name: &str) -> Option<Self> {
        match name {
            "Backlog/Ideas" => Some(TicketState::BacklogIdeas),
            "In Scope" => Some(TicketState::InScope),
            "Investigation/Discussion" => Some(TicketState::InvestigationDiscussion),
            "In Progress" => Some(TicketState::InProgress),
            "Pending Release" => Some(TicketState::PendingRelease),
            "Demo/Final Approval" => Some(TicketState::DemoFinalApproval),
            "Done" => Some(TicketState::Done),
            _ => None,
        }
    }
}
