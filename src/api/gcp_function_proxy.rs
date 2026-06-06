use google_cloud_storage::client::google_cloud_auth::credentials::CredentialsFile;
use google_cloud_storage::client::google_cloud_auth::idtoken::IdTokenSourceConfig;

use super::client::{DbtApiClient, check_ping_ok};
use super::proxy::{self, ProxyAuth};

/// GCP Cloud Function proxy connection. Endpoints/request shape match
/// [`super::normal_proxy::NormalProxyClient`]; the difference is authorization.
/// When `auth_with_service_account` is set we mint a GCP ID token for the
/// function (using the service account at `service_account_path`) and send it as
/// a `Bearer` token; otherwise no auth header is sent.
pub struct GcpFunctionProxyClient {
    http: reqwest::Client,
    endpoint_url: String,
    auth_with_service_account: bool,
    service_account_path: Option<String>,
}

impl GcpFunctionProxyClient {
    pub fn new(
        http: reqwest::Client,
        endpoint_url: String,
        auth_with_service_account: bool,
        service_account_path: Option<String>,
    ) -> Self {
        Self {
            http,
            endpoint_url,
            auth_with_service_account,
            service_account_path,
        }
    }

    /// Mints a GCP ID token for the Cloud Function (the function's
    /// `endpoint_url` is the audience), using the configured service account.
    async fn id_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let path =
            crate::gcp::client::resolve_service_account_path(self.service_account_path.as_deref())?;
        let credentials =
            CredentialsFile::new_from_file(path.to_str().unwrap().to_string()).await?;
        let token_source = IdTokenSourceConfig::new()
            .with_credentials(credentials)
            .build(&self.endpoint_url)
            .await?;
        Ok(token_source.token().await?.access_token)
    }

    /// The authorization for this proxy: a minted `Bearer` ID token when
    /// `auth_with_service_account` is set, otherwise none.
    async fn auth(&self) -> Result<ProxyAuth, Box<dyn std::error::Error>> {
        if self.auth_with_service_account {
            Ok(ProxyAuth::Bearer(self.id_token().await?))
        } else {
            Ok(ProxyAuth::None)
        }
    }
}

impl DbtApiClient for GcpFunctionProxyClient {
    async fn ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/ping", self.endpoint_url.trim_end_matches('/'));
        let resp = self.auth().await?.apply(self.http.get(url)).send().await?;
        check_ping_ok(resp)
    }

    async fn get_runs_queue(
        &self,
        project_name: &str,
    ) -> Result<crate::models::runs::RunsQueue, Box<dyn std::error::Error>> {
        proxy::get_runs_queue(&self.http, &self.endpoint_url, self.auth().await?, project_name).await
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
            &self.endpoint_url,
            self.auth().await?,
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
        proxy::check_run_status(&self.http, &self.endpoint_url, self.auth().await?, run_id).await
    }

    async fn cancel_run(
        &self,
        _project_name: &str,
        run_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        proxy::cancel_run(&self.http, &self.endpoint_url, self.auth().await?, run_id).await
    }
}

#[cfg(test)]
mod tests {
    // Only the `auth_with_service_account = false` path is covered here: the
    // service-account path mints a real Google-signed ID token, which can't be
    // exercised against a mock server.
    use super::*;
    use httpmock::prelude::HttpMockRequest;
    use httpmock::{Method::GET, MockServer};
    use serial_test::serial;

    /// True when the request carries no `Authorization` header (case-insensitive).
    fn no_auth_header(req: &HttpMockRequest) -> bool {
        match &req.headers {
            Some(headers) => !headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("authorization")),
            None => true,
        }
    }

    /// True when the request carries a non-empty `Bearer` token in its
    /// `Authorization` header. Does not validate the token itself.
    fn has_bearer_token(req: &HttpMockRequest) -> bool {
        match &req.headers {
            Some(headers) => headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("authorization")
                    && value.len() > "Bearer ".len()
                    && value.starts_with("Bearer ")
            }),
            None => false,
        }
    }

    /// Path to a real service account key, sourced from `TEST_SERVICE_ACCOUNT_PATH`
    /// (define it in `.env.test` or the environment). Minting an ID token below
    /// requires a valid service account and network access to Google.
    fn service_account_path() -> String {
        let _ = dotenvy::from_filename(".env.test");
        std::env::var("TEST_SERVICE_ACCOUNT_PATH").expect(
            "TEST_SERVICE_ACCOUNT_PATH must be set (define it in .env.test or the environment)",
        )
    }

    fn client(server: &MockServer) -> GcpFunctionProxyClient {
        GcpFunctionProxyClient::new(reqwest::Client::new(), server.base_url(), false, None)
    }

    #[tokio::test]
    async fn ping_without_service_account_sends_no_auth_header() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/ping").matches(no_auth_header);
                then.status(200);
            })
            .await;

        let result = client(&server).ping().await;

        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn ping_errors_on_non_200() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/ping");
                then.status(500);
            })
            .await;

        let result = client(&server).ping().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    #[serial]
    async fn ping_with_service_account_sends_bearer_token() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/ping").matches(has_bearer_token);
                then.status(200);
            })
            .await;

        let client = GcpFunctionProxyClient::new(
            reqwest::Client::new(),
            server.base_url(),
            true,
            Some(service_account_path()),
        );

        let result = client.ping().await;

        assert!(result.is_ok(), "ping failed: {:?}", result.err());
        mock.assert_async().await;
    }
}
