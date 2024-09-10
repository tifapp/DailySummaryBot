use std::{collections::HashMap, env};
use serde::{Deserialize, Serialize};
use reqwest::Client;
use anyhow::{Result, Error};
use crate::{sprint_summary::{ticket::{TicketDetails, TicketLink}, ticket_label::TicketLabel, ticket_state::TicketState}, tracing::info};

use super::TicketDetailsClient;

#[derive(Debug, Serialize, Deserialize)]
struct TrelloAttachment {
    name: String,
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
struct TrelloCardLink {
    id: String,
    name: String,
    url: String,
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

impl TicketDetailsClient for Client {
    async fn fetch_ticket_details(&self) -> Result<Vec<TicketDetails>, Error> {
        let lists = fetch_trello_lists(&self).await?;
        let list_name_to_ticket_state_map: HashMap<_, _> = lists.into_iter().map(|list| (list.id, TicketState::from_str(&list.name))).collect();
        let cards = fetch_trello_cards(&self).await?;

        let card_url_to_name_map: HashMap<String, String> = cards.iter()
            .map(|card| (card.url.clone(), card.name.clone()))
            .collect();
        
        Ok(cards.into_iter().filter_map(|card| {
            list_name_to_ticket_state_map.get(&card.idList).and_then(|list_name_option| {
                list_name_option.as_ref().map(|state| TicketDetails {
                    id: card.id.clone(),
                    name: card.name,
                    member_ids: card.idMembers,
                    state: state.clone(),
                    url: card.url,
                    has_labels: !card.labels.is_empty(),
                    has_description: card.desc.as_ref().map_or(false, |d| !d.is_empty()),
                    labels: card.labels.iter().filter_map(|label| TicketLabel::from_str(&label.name)).collect(),
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
                    dependency_of: card.attachments.iter()
                        .find_map(|attachment| {
                            if attachment.url.contains("trello.com/c") {
                                Some(TicketLink {
                                    name: card_url_to_name_map.get(&attachment.url).unwrap_or(&attachment.name).clone(),
                                    url: attachment.url.clone()
                                })
                            } else {
                                None
                            }
                        }),
                })
            })
        }).collect::<Vec<TicketDetails>>())
    }    
}
