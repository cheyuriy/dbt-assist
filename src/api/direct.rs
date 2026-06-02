use super::client::{DbtApiClient, check_ping_ok};

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
        let url = format!("{}/v2/accounts", self.url.trim_end_matches('/'));
        let resp = self.http.get(url).bearer_auth(&self.token).send().await?;
        check_ping_ok(resp)
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
