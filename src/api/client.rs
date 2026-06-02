use crate::models::config::{AppConfig, DbtApiConnection};

use super::direct::DirectClient;
use super::gcp_function_proxy::GcpFunctionProxyClient;
use super::normal_proxy::NormalProxyClient;

/// Treats a `200 OK` response as success for a ping; any other status is an
/// error. Shared by the connectors' `ping` implementations.
pub(crate) fn check_ping_ok(resp: reqwest::Response) -> Result<(), Box<dyn std::error::Error>> {
    if resp.status() == reqwest::StatusCode::OK {
        Ok(())
    } else {
        Err(format!("ping failed with status {}", resp.status()).into())
    }
}

/// Generic interface to the dbt API, regardless of how we reach it (directly,
/// via a plain proxy, or via a GCP Cloud Function proxy).
///
/// Return types are intentionally minimal for now (opaque ids/status as
/// `String`); they will be replaced with real domain types once the methods
/// are implemented.
// Only `ping` is wired up so far; the other four methods are still stubs and
// not yet called. Remove this allow as they get implemented and used.
#[allow(dead_code)]
pub trait DbtApiClient {
    /// Connectivity/health check against the API (or proxy).
    async fn ping(&self) -> Result<(), Box<dyn std::error::Error>>;

    /// Returns the queue of runs for our special job (run ids for now).
    async fn get_runs_queue(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;

    /// Creates a new run of our special job and returns its id.
    async fn create_run(&self) -> Result<String, Box<dyn std::error::Error>>;

    /// Checks the status of a run of our special job.
    async fn check_run_status(&self, run_id: &str) -> Result<String, Box<dyn std::error::Error>>;

    /// Cancels a run of our special job.
    async fn cancel_run(&self, run_id: &str) -> Result<(), Box<dyn std::error::Error>>;
}

/// Static-dispatch wrapper over the concrete client for each connection type.
/// Built from [`AppConfig`] via [`DbtApi::from_config`].
pub enum DbtApi {
    Direct(DirectClient),
    NormalProxy(NormalProxyClient),
    GcpFunctionProxy(GcpFunctionProxyClient),
}

impl DbtApi {
    /// Builds the appropriate client variant from the connection settings in
    /// `config`. A single shared `reqwest::Client` is created here and handed
    /// to the concrete client.
    pub fn from_config(config: &AppConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let http = reqwest::Client::new();

        let api = match &config.dbt_api_connection {
            DbtApiConnection::Direct {
                dbt_api_url,
                dbt_api_token,
            } => DbtApi::Direct(DirectClient::new(
                http,
                dbt_api_url.clone(),
                dbt_api_token.clone(),
            )),

            DbtApiConnection::NormalProxy {
                proxy_url,
                proxy_token,
            } => DbtApi::NormalProxy(NormalProxyClient::new(
                http,
                proxy_url.clone(),
                proxy_token.clone(),
            )),

            DbtApiConnection::GcpFunctionProxy {
                endpoint_url,
                auth_with_service_account,
            } => DbtApi::GcpFunctionProxy(GcpFunctionProxyClient::new(
                http,
                endpoint_url.clone(),
                *auth_with_service_account,
                config.service_account_path.clone(),
            )),
        };

        Ok(api)
    }
}

impl DbtApiClient for DbtApi {
    async fn ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            DbtApi::Direct(c) => c.ping().await,
            DbtApi::NormalProxy(c) => c.ping().await,
            DbtApi::GcpFunctionProxy(c) => c.ping().await,
        }
    }

    async fn get_runs_queue(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        match self {
            DbtApi::Direct(c) => c.get_runs_queue().await,
            DbtApi::NormalProxy(c) => c.get_runs_queue().await,
            DbtApi::GcpFunctionProxy(c) => c.get_runs_queue().await,
        }
    }

    async fn create_run(&self) -> Result<String, Box<dyn std::error::Error>> {
        match self {
            DbtApi::Direct(c) => c.create_run().await,
            DbtApi::NormalProxy(c) => c.create_run().await,
            DbtApi::GcpFunctionProxy(c) => c.create_run().await,
        }
    }

    async fn check_run_status(&self, run_id: &str) -> Result<String, Box<dyn std::error::Error>> {
        match self {
            DbtApi::Direct(c) => c.check_run_status(run_id).await,
            DbtApi::NormalProxy(c) => c.check_run_status(run_id).await,
            DbtApi::GcpFunctionProxy(c) => c.check_run_status(run_id).await,
        }
    }

    async fn cancel_run(&self, run_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            DbtApi::Direct(c) => c.cancel_run(run_id).await,
            DbtApi::NormalProxy(c) => c.cancel_run(run_id).await,
            DbtApi::GcpFunctionProxy(c) => c.cancel_run(run_id).await,
        }
    }
}
