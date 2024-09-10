use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Deserialize, Serialize)]
pub enum TicketLabel {
    Goal,
    FrontEnd,
    BackEnd,
    Infra,
    Bug,
    Minor,
    Blocked,
}

impl TicketLabel {
    pub fn from_str(name: &str) -> Option<Self> {
        match name {
            "Goal" => Some(TicketLabel::Goal),
            "Front-End" => Some(TicketLabel::FrontEnd),
            "Back-End" => Some(TicketLabel::BackEnd),
            "Infra" => Some(TicketLabel::Infra),
            "Bug" => Some(TicketLabel::Bug),
            "Minor" => Some(TicketLabel::Minor),
            "Blocked" => Some(TicketLabel::Blocked),
            _ => None,
        }
    }

    pub fn emoji(&self) -> &str {
        match self {
            TicketLabel::Goal => "🏁",
            TicketLabel::FrontEnd => "📱",
            TicketLabel::BackEnd => "🌐",
            TicketLabel::Infra => "🔧",
            TicketLabel::Bug => "🐛",
            TicketLabel::Minor => "🪶",
            TicketLabel::Blocked => "🚧",
        }
    }
}
