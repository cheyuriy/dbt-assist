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

    async fn get_runs_queue(
        &self,
        _project_name: &str,
    ) -> Result<crate::models::runs::RunsQueue, Box<dyn std::error::Error>> {
        todo!()
    }

    async fn create_run(&self) -> Result<String, Box<dyn std::error::Error>> {
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
    use httpmock::prelude::HttpMockRequest;
    use httpmock::{Method::GET, MockServer};

    fn client(server: &MockServer, token: Option<&str>) -> NormalProxyClient {
        NormalProxyClient::new(
            reqwest::Client::new(),
            server.base_url(),
            token.map(str::to_string),
        )
    }

    /// True when the request carries no `Authorization` header (case-insensitive).
    fn no_auth_header(req: &HttpMockRequest) -> bool {
        match &req.headers {
            Some(headers) => !headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("authorization")),
            None => true,
        }
    }

    #[tokio::test]
    async fn ping_with_token_sends_apikey_header() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/ping")
                    .header("authorization", "ApiKey secret-token");
                then.status(200);
            })
            .await;

        let result = client(&server, Some("secret-token")).ping().await;

        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn ping_without_token_sends_no_auth_header() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/ping").matches(no_auth_header);
                then.status(200);
            })
            .await;

        let result = client(&server, None).ping().await;

        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn ping_errors_on_non_200() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/ping");
                then.status(503);
            })
            .await;

        let result = client(&server, None).ping().await;

        assert!(result.is_err());
    }
}
