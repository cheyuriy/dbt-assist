use super::client::{DbtApiClient, check_ping_ok};

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
        let url = format!("{}/ping", self.url.trim_end_matches('/'));
        let mut request = self.http.get(url);
        if let Some(token) = &self.token {
            request = request.header(reqwest::header::AUTHORIZATION, format!("ApiKey {token}"));
        }
        let resp = request.send().await?;
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
