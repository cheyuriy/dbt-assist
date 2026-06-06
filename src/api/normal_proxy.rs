use super::client::{DbtApiClient, check_ping_ok};
use super::proxy::{self, ProxyAuth};

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

    /// The authorization for this proxy: `ApiKey <token>` when a token is set,
    /// otherwise none.
    fn auth(&self) -> ProxyAuth {
        match &self.token {
            Some(token) => ProxyAuth::ApiKey(token.clone()),
            None => ProxyAuth::None,
        }
    }
}

impl DbtApiClient for NormalProxyClient {
    async fn ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/ping", self.url.trim_end_matches('/'));
        let resp = self.auth().apply(self.http.get(url)).send().await?;
        check_ping_ok(resp)
    }

    async fn get_runs_queue(
        &self,
        project_name: &str,
    ) -> Result<crate::models::runs::RunsQueue, Box<dyn std::error::Error>> {
        proxy::get_runs_queue(&self.http, &self.url, self.auth(), project_name).await
    }

    async fn create_run(
        &self,
        project_name: &str,
        select: &str,
        exclude: Option<&str>,
        full_refresh: Option<bool>,
        turbo: bool,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        proxy::create_run(
            &self.http,
            &self.url,
            self.auth(),
            project_name,
            select,
            exclude,
            full_refresh,
            turbo,
        )
        .await
    }

    async fn check_run_status(
        &self,
        _project_name: &str,
        run_id: &str,
    ) -> Result<crate::models::runs::RunStatus, Box<dyn std::error::Error>> {
        proxy::check_run_status(&self.http, &self.url, self.auth(), run_id).await
    }

    async fn cancel_run(
        &self,
        _project_name: &str,
        run_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        proxy::cancel_run(&self.http, &self.url, self.auth(), run_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::HttpMockRequest;
    use httpmock::{
        Method::{DELETE, GET, POST},
        MockServer,
    };

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

    #[tokio::test]
    async fn create_run_posts_body_and_returns_id() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/runs/manual")
                    .header("authorization", "ApiKey secret-token")
                    .json_body(serde_json::json!({
                        "select": "tag:nightly",
                        "project_name": "analytics",
                        "exclude": "model_x",
                        "full_refresh": true,
                        "turbo": false,
                    }));
                then.status(201).json_body(serde_json::json!({"run_id": 1234}));
            })
            .await;

        let result = client(&server, Some("secret-token"))
            .create_run("analytics", "tag:nightly", Some("model_x"), Some(true), false)
            .await;

        assert_eq!(result.expect("create run"), 1234);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn create_run_omits_exclude_and_nulls_full_refresh_when_absent() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/runs/manual").json_body(serde_json::json!({
                    "select": "*",
                    "project_name": "analytics",
                    "full_refresh": null,
                    "turbo": true,
                }));
                then.status(201).json_body(serde_json::json!({"run_id": 7}));
            })
            .await;

        let result = client(&server, None)
            .create_run("analytics", "*", None, None, true)
            .await;

        assert_eq!(result.expect("create run"), 7);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn create_run_surfaces_error_message() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/runs/manual");
                then.status(500).json_body(serde_json::json!({"message": "boom"}));
            })
            .await;

        let err = client(&server, None)
            .create_run("analytics", "*", None, None, false)
            .await
            .expect_err("should fail");

        assert!(err.to_string().contains("boom"), "got: {err}");
    }

    #[tokio::test]
    async fn get_runs_queue_parses_runs() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/jobs/manual/queue")
                    .json_body(serde_json::json!({"project_name": "analytics"}));
                then.status(200).json_body(serde_json::json!({
                    "active_runs": 1,
                    "runs": [{
                        "id": 99,
                        "status": 2,
                        "run_duration_humanized": "1m",
                        "queued_duration_humanized": "0s",
                        "trigger": {"cause": "a@b.c"},
                        "job": {"execute_steps": ["dbt build"]},
                    }],
                }));
            })
            .await;

        let queue = client(&server, None)
            .get_runs_queue("analytics")
            .await
            .expect("queue");

        assert_eq!(queue.runs.len(), 1);
        assert_eq!(queue.runs[0].id, 99);
        assert_eq!(queue.runs[0].status_label(), "Running");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn check_run_status_parses_status() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/runs/42");
                then.status(200).json_body(serde_json::json!({
                    "in_progress": false,
                    "is_complete": true,
                    "is_success": true,
                    "is_error": false,
                    "is_cancelled": false,
                    "duration": "00:01:00",
                    "run_steps": [{
                        "name": "build",
                        "index": 1,
                        "status_humanized": "Success",
                        "logs": "ok",
                        "debug_logs": null,
                    }],
                }));
            })
            .await;

        let status = client(&server, None)
            .check_run_status("analytics", "42")
            .await
            .expect("status");

        assert_eq!(status.status_label(), "Success");
        assert_eq!(status.run_steps.len(), 1);
        assert_eq!(status.run_steps[0].logs.as_deref(), Some("ok"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn cancel_run_succeeds_on_200() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(DELETE).path("/runs/9");
                then.status(200);
            })
            .await;

        let result = client(&server, None).cancel_run("analytics", "9").await;

        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn cancel_run_surfaces_error_message() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(DELETE).path("/runs/9");
                then.status(404).json_body(serde_json::json!({"message": "not found"}));
            })
            .await;

        let err = client(&server, None)
            .cancel_run("analytics", "9")
            .await
            .expect_err("should fail");

        assert!(err.to_string().contains("not found"), "got: {err}");
    }
}
