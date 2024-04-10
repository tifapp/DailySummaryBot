use std::{env};
use serde::{Deserialize, Serialize};
use reqwest::{Client, Error};
use anyhow::Result;
use crate::tracing::info;

#[derive(Debug, Serialize, Deserialize)]
pub struct Attachment {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrelloCard {
    pub name: String,
    pub idMembers: Vec<String>,
    pub idList: String,
    pub url: String,
    pub desc: Option<String>,
    pub attachments: Vec<Attachment>
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

pub async fn fetch_board_name(client: &Client, board_id: &str) -> Result<String, Error> {
    let trello_api_key = env::var("TRELLO_API_KEY").expect("TRELLO_API_KEY environment variable should exist");
    let trello_api_token = env::var("TRELLO_API_TOKEN").expect("TRELLO_API_TOKEN environment variable should exist");

    let board_url = format!("https://api.trello.com/1/boards/{}?key={}&token={}", board_id, trello_api_key, trello_api_token);

    let response = client.get(&board_url)
        .send()
        .await?
        .error_for_status()?;

    let board: Board = response.json().await?;
    info!("Fetched board name: {}", board.name);
    Ok(board.name)
}


pub async fn fetch_trello_lists(client: &Client, trello_board_id: &str) -> Result<Vec<TrelloList>, Error> {
    let trello_api_key = env::var("TRELLO_API_KEY").expect("TRELLO_API_KEY environment variable should exist");
    let trello_api_token = env::var("TRELLO_API_TOKEN").expect("TRELLO_API_TOKEN environment variable should exist");

    let lists_url = format!("https://api.trello.com/1/boards/{}/lists?key={}&token={}", trello_board_id, trello_api_key, trello_api_token);

    let response = client.get(&lists_url)
        .send()
        .await
        .expect("Failed to fetch Trello lists");

    info!("Trello cards response body: {:?}", response);

    Ok(response
        .json::<Vec<TrelloList>>()
        .await
        .expect("Failed to parse Trello lists")
    )
}

pub async fn fetch_trello_cards(client: &Client, trello_board_id: &str) -> Result<Vec<TrelloCard>, Error> {
    let trello_api_key = env::var("TRELLO_API_KEY").expect("TRELLO_API_KEY environment variable should exist");
    let trello_api_token = env::var("TRELLO_API_TOKEN").expect("TRELLO_API_TOKEN environment variable should exist");

    let cards_url = format!("https://api.trello.com/1/boards/{}/cards?fields=name,desc,idList,idMembers,url&attachments=true&key={}&token={}", trello_board_id, trello_api_key, trello_api_token);

    let response = client.get(&cards_url)
        .send()
        .await
        .expect("Failed to fetch Trello cards");
    
    info!("Trello lists response body: {:?}", response);

    Ok(response
        .json::<Vec<TrelloCard>>()
        .await
        .expect("Failed to parse Trello cards")
    )
}
