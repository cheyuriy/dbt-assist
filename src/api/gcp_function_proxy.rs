use google_cloud_storage::client::google_cloud_auth::credentials::CredentialsFile;
use google_cloud_storage::client::google_cloud_auth::idtoken::IdTokenSourceConfig;

use super::client::{DbtApiClient, check_ping_ok};

/// GCP Cloud Function proxy connection. Endpoints/request shape match
/// [`super::normal_proxy::NormalProxyClient`]; the difference is authorization.
/// When `auth_with_service_account` is set we mint a GCP ID token for the
/// function (using the service account at `service_account_path`); otherwise the
/// same `ApiKey` token scheme as the normal proxy applies.
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
}

impl DbtApiClient for GcpFunctionProxyClient {
    async fn ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/ping", self.endpoint_url.trim_end_matches('/'));
        let mut request = self.http.get(url);
        if self.auth_with_service_account {
            request = request.bearer_auth(self.id_token().await?);
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
