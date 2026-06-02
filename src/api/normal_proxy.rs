// Stub implementation; fields are populated but not yet read until the methods
// are implemented in a follow-up step.
#![allow(dead_code)]

use super::client::DbtApiClient;

/// Plain proxy connection: requests go to the user's proxy `url`. If `token` is
/// set it is sent as `Authorization: ApiKey <token>`; otherwise no auth header
/// is used.
pub struct NormalProxyClient {
    http: reqwest::Client,
    url: String,
    token: Option<String>,
}

impl NormalProxyClient {
    pub fn new(http: reqwest::Client, url: String, token: Option<String>) -> Self {
        Self { http, url, token }
    }
}

impl DbtApiClient for NormalProxyClient {
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
