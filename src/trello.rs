use std::{collections::HashMap, env, sync::Arc};
use serde::{Deserialize, Serialize};
use reqwest::{Client, Error};
use anyhow::Result;
use crate::tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct TrelloAttachment {
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrelloLabel {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrelloBadges {    
    checkItems: u32,
    checkItemsChecked: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrelloCard {
    id: String,
    name: String,
    idMembers: Vec<String>,
    idList: String,
    url: String,
    labels: Vec<TrelloLabel>,
    desc: Option<String>,
    attachments: Vec<TrelloAttachment>,
    badges: TrelloBadges,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrelloList {
    id: String,
    name: String,
}

//generic inteface to work with any ticket tracking system
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
    pub is_backlogged: bool,
    pub checklist_items: u32,
    pub checked_checklist_items: u32,
    pub pr_url: Option<String>,
}

async fn fetch_trello_lists(client: &Client, trello_board_id: &str) -> Result<Vec<TrelloList>, Error> {
    let trello_api_key = env::var("TRELLO_API_KEY").expect("TRELLO_API_KEY environment variable should exist");
    let trello_api_token = env::var("TRELLO_API_TOKEN").expect("TRELLO_API_TOKEN environment variable should exist");

    let lists_url = format!("https://api.trello.com/1/boards/{}/lists?key={}&token={}", trello_board_id, trello_api_key, trello_api_token);

    let response = client.get(&lists_url)
        .send()
        .await
        .expect("Failed to fetch Trello lists");
    
    let body = response.text().await.expect("Failed to read response body");

    info!("Trello cards response body: {}", body);

    Ok(serde_json::from_str(&body).expect("Failed to parse Trello lists"))
}

async fn fetch_trello_cards(client: &Client, trello_board_id: &str) -> Result<Vec<TrelloCard>, Error> {
    let trello_api_key = env::var("TRELLO_API_KEY").expect("TRELLO_API_KEY environment variable should exist");
    let trello_api_token = env::var("TRELLO_API_TOKEN").expect("TRELLO_API_TOKEN environment variable should exist");

    let cards_url = format!("https://api.trello.com/1/boards/{}/cards?fields=badges,name,desc,idList,idMembers,url,labels&attachments=true&key={}&token={}", trello_board_id, trello_api_key, trello_api_token);

    let response = client.get(&cards_url)
        .send()
        .await
        .expect("Failed to fetch Trello cards");
    
    let body = response.text().await.expect("Failed to read response body");

    info!("Trello cards response body: {}", body);

    Ok(serde_json::from_str(&body).expect("Failed to parse Trello cards"))
}

pub async fn fetch_ticket_details(client: Arc<Client>, trello_board_id: &str) -> Result<Vec<TicketDetails>, Error> {
    let lists = fetch_trello_lists(&client, trello_board_id).await?;
    let list_name_map = Arc::new(lists.into_iter().map(|list| (list.id, list.name)).collect::<HashMap<_, _>>());
    let cards = fetch_trello_cards(&client, trello_board_id).await?;
    
    Ok(cards.into_iter().map(|card| {
        let list_name_map_clone = Arc::clone(&list_name_map);

        TicketDetails {
            id: card.id.clone(),
            name: card.name,
            member_ids: card.idMembers,
            list_name: list_name_map_clone.get(&card.idList).unwrap_or(&"None".to_string()).clone(),
            url: card.url,
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
            checklist_items: card.badges.checkItems,
            checked_checklist_items: card.badges.checkItemsChecked,
            pr_url: card.attachments.iter()
                .find_map(|attachment| {
                    if attachment.url.contains("github.com") && attachment.url.contains("/pull/") {
                        Some(attachment.url.clone())
                    } else {
                        None
                    }
                }),
            is_backlogged: false,
        }
    }).collect::<Vec<TicketDetails>>())
}
