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

    async fn get_runs_queue(
        &self,
        _project_name: &str,
    ) -> Result<crate::models::runs::RunsQueue, Box<dyn std::error::Error>> {
        todo!()
    }

    async fn create_run(
        &self,
        _project_name: &str,
        _select: &str,
        _exclude: Option<&str>,
        _full_refresh: Option<bool>,
        _turbo: bool,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        todo!()
    }

    async fn check_run_status(
        &self,
        _project_name: &str,
        _run_id: &str,
    ) -> Result<crate::models::runs::RunStatus, Box<dyn std::error::Error>> {
        todo!()
    }

    async fn cancel_run(
        &self,
        _project_name: &str,
        _run_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::{Method::GET, MockServer};

    fn client(server: &MockServer, token: &str) -> DirectClient {
        DirectClient::new(reqwest::Client::new(), server.base_url(), token.to_string())
    }

    #[tokio::test]
    async fn ping_succeeds_on_200() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/v2/accounts")
                    .header("authorization", "Bearer secret-token");
                then.status(200);
            })
            .await;

        let result = client(&server, "secret-token").ping().await;

        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn ping_errors_on_non_200() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/accounts");
                then.status(401);
            })
            .await;

        let result = client(&server, "secret-token").ping().await;

        assert!(result.is_err());
    }
}
