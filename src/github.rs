use std::env;
use serde::{Serialize, Deserialize};
use anyhow::Result;
use reqwest::{Client, Error};
use crate::tracing::info;

#[derive(Deserialize)]
struct GithubHead {
    sha: String,
}

#[derive(Deserialize)]
struct GithubPullRequest {
    head: GithubHead,
    comments: u32,
    draft: bool,
}

#[derive(Deserialize, Debug)]
struct GithubCheckRun {
    name: String,
    conclusion: Option<String>, // "success", "failure", "neutral", "cancelled", "timed_out", "action_required", or null if still in progress
    status: String, // "queued", "in_progress", or "completed"
    details_url: String,
}

#[derive(Deserialize, Debug)]
struct GithubCheckRuns {
    check_runs: Vec<GithubCheckRun>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckRunDetails {
    pub name: String,
    pub details_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub pr_url: String,
    pub state: String,
    pub comments: u32,
    pub is_draft: bool,
    pub action_required_check_runs: Vec<CheckRunDetails>,
    pub failing_check_runs: Vec<CheckRunDetails>,
}

fn check_overall_status(check_runs: &GithubCheckRuns) -> (String, Vec<CheckRunDetails>, Vec<CheckRunDetails>) {
    let mut failing_check_runs = Vec::new();
    let mut action_required_check_runs = Vec::new();

    for check_run in &check_runs.check_runs {
        match check_run.conclusion.as_deref() {
            Some("failure") => failing_check_runs.push(CheckRunDetails { 
                name: check_run.name.clone(), 
                details_url: check_run.details_url.clone(),
            }),
            Some("action_required") | None => action_required_check_runs.push(CheckRunDetails {
                name: check_run.name.clone(),
                details_url: check_run.details_url.clone(),
            }),
            _ => (),
        }
    }

    let state = if !failing_check_runs.is_empty() {
        "failure".to_string()
    } else if action_required_check_runs.is_empty() {
        "success".to_string()
    } else {
        "action_required".to_string()
    };

    (state, failing_check_runs, action_required_check_runs)
}

pub async fn fetch_pr_details(client: &Client, pr_url: &str) -> Result<PullRequest, Error> {
    let github_token = env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN environment variable should exist");

    info!("Going to get status checks from Github PR: {:?}", pr_url);

    let re = regex::Regex::new(r"github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/pull/(?P<number>\d+)").unwrap();
    let caps = re.captures(pr_url).expect("Failed to parse GitHub PR URL");

    let owner = caps.name("owner").unwrap().as_str();
    let repo = caps.name("repo").unwrap().as_str();
    let number = caps.name("number").unwrap().as_str();

    let pr_details_url = format!("https://api.github.com/repos/{}/{}/pulls/{}", owner, repo, number);
    
    info!("Fetching details for PR: {:?}", pr_details_url);

    let pr_response = client.get(&pr_details_url)
        .bearer_auth(github_token.clone())
        .header("User-Agent", "daily_summary_request")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?
        .error_for_status()?;
    
    info!("Github pr details response body: {:?}", pr_response);

    let pr: GithubPullRequest = pr_response.json().await?;

    let checks_url = format!("https://api.github.com/repos/{}/{}/commits/{}/check-runs", owner, repo, pr.head.sha);

    info!("Fetching status checks for commit: {:?}", checks_url);

    let checks_response = client.get(&checks_url)
        .bearer_auth(github_token.clone())
        .header("User-Agent", "daily_summary_request")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await;
        
    info!("Github status checks response body: {:?}", checks_response);

    let (state, failing_check_runs, action_required_check_runs) = match checks_response {
        Ok(response) => {
            if response.status() == reqwest::StatusCode::FORBIDDEN {
                // Directly set the state to "success" if a 403 Forbidden response is encountered
                ("success".to_string(), vec![], vec![])
            } else {
                // Process normally if response is not 403 Forbidden
                let checks_response = response.error_for_status()?;
                let checks = checks_response.json::<GithubCheckRuns>().await?;
                check_overall_status(&checks)
            }
        },
        Err(e) => {
            // Handle other types of errors, e.g., network errors or non-403 HTTP errors
            return Err(e);
        }
    };

    info!("Github pr state: {:?}", state);
    
    Ok(
        PullRequest {
            pr_url: pr_url.to_string(),
            state,
            comments: pr.comments,
            is_draft: pr.draft,
            action_required_check_runs,
            failing_check_runs
        }
    )
}
