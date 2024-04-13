use std::{env};
use serde::{Deserialize, Serialize};
use reqwest::{Client, Error};
use anyhow::Result;
use crate::tracing::info;

#[derive(Debug, Serialize, Deserialize)]
pub struct TrelloAttachment {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrelloLabel {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrelloBadges {    
    pub checkItems: u32,
    pub checkItemsChecked: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrelloCard {
    pub id: String,
    pub name: String,
    pub idMembers: Vec<String>,
    pub idList: String,
    pub url: String,
    pub labels: Vec<TrelloLabel>,
    pub desc: Option<String>,
    pub attachments: Vec<TrelloAttachment>,
    pub badges: TrelloBadges,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrelloList {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Board {
    pub name: String,
}

pub async fn fetch_trello_lists(client: &Client, trello_board_id: &str) -> Result<Vec<TrelloList>, Error> {
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

pub async fn fetch_trello_cards(client: &Client, trello_board_id: &str) -> Result<Vec<TrelloCard>, Error> {
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
