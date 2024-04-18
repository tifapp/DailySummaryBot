use std::{collections::HashMap, env, sync::Arc};
use serde::{Deserialize, Serialize};
use reqwest::{Client, Error};
use anyhow::Result;
use crate::{sprint_summary::ticket::TicketDetails, tracing::info};

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

async fn fetch_trello_lists(client: &Client) -> Result<Vec<TrelloList>, Error> {
    let trello_board_id = env::var("TRELLO_BOARD_ID").expect("TRELLO_BOARD_ID environment variable should exist");
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

async fn fetch_trello_cards(client: &Client) -> Result<Vec<TrelloCard>, Error> {
    let trello_board_id = env::var("TRELLO_BOARD_ID").expect("TRELLO_BOARD_ID environment variable should exist");
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

pub trait TicketDetailsClient {
    async fn fetch_ticket_details(&self) -> Result<Vec<TicketDetails>, Error>;
}

impl TicketDetailsClient for Client {
    async fn fetch_ticket_details(&self) -> Result<Vec<TicketDetails>, Error> {
        let lists = fetch_trello_lists(&self).await?;
        let list_name_map = Arc::new(lists.into_iter().map(|list| (list.id, list.name)).collect::<HashMap<_, _>>());
        let cards = fetch_trello_cards(&self).await?;
        
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
            }
        }).collect::<Vec<TicketDetails>>())
    }    
}