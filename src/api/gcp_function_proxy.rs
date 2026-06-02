// Stub implementation; fields are populated but not yet read until the methods
// are implemented in a follow-up step.
#![allow(dead_code)]

use super::client::DbtApiClient;

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
}

impl DbtApiClient for GcpFunctionProxyClient {
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
