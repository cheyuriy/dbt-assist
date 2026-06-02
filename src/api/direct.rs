// Stub implementation; fields are populated but not yet read until the methods
// are implemented in a follow-up step.
#![allow(dead_code)]

use super::client::DbtApiClient;

/// Direct connection to the dbt API: requests go to `url` and are authorized
/// with `token` as a `Bearer` token.
pub struct DirectClient {
    http: reqwest::Client,
    url: String,
    token: String,
}

impl DirectClient {
    pub fn new(http: reqwest::Client, url: String, token: String) -> Self {
        Self { http, url, token }
    }
}

impl DbtApiClient for DirectClient {
    async fn ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    async fn get_runs_queue(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        todo!()
    }

    async fn create_run(&self) -> Result<String, Box<dyn std::error::Error>> {
        todo!()
    }

    async fn check_run_status(&self, _run_id: &str) -> Result<String, Box<dyn std::error::Error>> {
        todo!()
    }

    async fn cancel_run(&self, _run_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }
}
