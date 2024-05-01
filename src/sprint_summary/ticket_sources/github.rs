use std::env;
use serde::Deserialize;
use anyhow::{Result, Error, anyhow};
use reqwest::Client;
use crate::{sprint_summary::ticket::{CheckRunDetails, PullRequest}, tracing::info};

use super::PullRequestClient;

#[derive(Deserialize)]
struct GithubHead {
    sha: String,
}

#[derive(Deserialize)]
struct GithubPullRequest {
    head: GithubHead,
    comments: u32,
    draft: bool,
    merged: bool,
    mergeable: Option<bool>
}

#[derive(Deserialize, Debug)]
struct GithubCheckRun {
    name: String,
    conclusion: Option<String>,
    details_url: String,
}

#[derive(Deserialize, Debug)]
struct GithubCheckRuns {
    check_runs: Vec<GithubCheckRun>,
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

impl PullRequestClient for Client {
    async fn fetch_pr_details(&self, pr_url: &str) -> Result<PullRequest, Error> {
        let github_token = env::var("USER_GITHUB_TOKEN").expect("USER_GITHUB_TOKEN environment variable should exist");
    
        info!("Going to get status checks from Github PR: {:?}", pr_url);
    
        let re = regex::Regex::new(r"github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/pull/(?P<number>\d+)").unwrap();
        let caps = re.captures(pr_url).expect("Failed to parse GitHub PR URL");
    
        let owner = caps.name("owner").unwrap().as_str();
        let repo = caps.name("repo").unwrap().as_str();
        let number = caps.name("number").unwrap().as_str();
    
        let pr_details_url = format!("https://api.github.com/repos/{}/{}/pulls/{}", owner, repo, number);
        
        info!("Fetching details for PR: {:?}", pr_details_url);
    
        let pr_response = self.get(&pr_details_url)
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
    
        let checks_response = self.get(&checks_url)
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
                return Err(anyhow!(e));
            }
        };
    
        info!("Github pr state: {:?}", state);
        
        Ok(
            PullRequest {
                state,
                comments: pr.comments,
                merged: pr.merged,
                mergeable: pr.mergeable,
                is_draft: pr.draft,
                action_required_check_runs,
                failing_check_runs
            }
        )
    }    
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_checks_succeed() {
        let checks = GithubCheckRuns {
            check_runs: vec![
                GithubCheckRun {
                    name: "Build".to_string(),
                    details_url: "http://example.com/build".to_string(),
                    conclusion: Some("success".to_string()),
                },
            ],
        };

        let (state, failing, action_required) = check_overall_status(&checks);
        assert_eq!(state, "success");
        assert!(failing.is_empty());
        assert!(action_required.is_empty());
    }

    #[test]
    fn test_some_checks_fail() {
        let checks = GithubCheckRuns {
            check_runs: vec![
                GithubCheckRun {
                    name: "Build".to_string(),
                    details_url: "http://example.com/build".to_string(),
                    conclusion: Some("failure".to_string()),
                },
            ],
        };

        let (state, failing, action_required) = check_overall_status(&checks);
        assert_eq!(state, "failure");
        assert_eq!(failing[0], CheckRunDetails { name: "Build".to_string(), details_url: "http://example.com/build".to_string() });
        assert!(action_required.is_empty());
    }

    #[test]
    fn test_some_checks_require_action() {
        let checks = GithubCheckRuns {
            check_runs: vec![
                GithubCheckRun {
                    name: "Deploy".to_string(),
                    details_url: "http://example.com/deploy".to_string(),
                    conclusion: None,
                },
            ],
        };

        let (state, failing, action_required) = check_overall_status(&checks);
        assert_eq!(state, "action_required");
        assert!(failing.is_empty());
        assert_eq!(action_required[0], CheckRunDetails { name: "Deploy".to_string(), details_url: "http://example.com/deploy".to_string() });
    }

    #[test]
    fn test_mixed_results() {
        let checks = GithubCheckRuns {
            check_runs: vec![
                GithubCheckRun {
                    name: "Build".to_string(),
                    details_url: "http://example.com/build".to_string(),
                    conclusion: Some("failure".to_string()),
                },
                GithubCheckRun {
                    name: "Deploy".to_string(),
                    details_url: "http://example.com/deploy".to_string(),
                    conclusion: None,
                },
            ],
        };

        let (state, failing, action_required) = check_overall_status(&checks);
        assert_eq!(state, "failure");
        assert_eq!(failing[0], CheckRunDetails { name: "Build".to_string(), details_url: "http://example.com/build".to_string() });
        assert_eq!(action_required[0], CheckRunDetails { name: "Deploy".to_string(), details_url: "http://example.com/deploy".to_string() });
    }

    #[test]
    fn test_no_checks() {
        let checks = GithubCheckRuns {
            check_runs: vec![],
        };

        let (state, failing, action_required) = check_overall_status(&checks);
        assert_eq!(state, "success");
        assert!(failing.is_empty());
        assert!(action_required.is_empty());
    }
}
