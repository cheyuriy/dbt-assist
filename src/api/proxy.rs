use serde::{Deserialize, Serialize};

use crate::models::runs::{RunStatus, RunsQueue};

/// How to authorize a request to a proxy. The normal proxy and the GCP Cloud
/// Function proxy speak the same API and differ *only* here: the normal proxy
/// optionally sends HTTP `Basic` auth (username + password), the GCP proxy
/// optionally sends a minted `Bearer <id-token>`, and either may send nothing.
pub(crate) enum ProxyAuth {
    None,
    Basic { username: String, password: String },
    Bearer(String),
}

impl ProxyAuth {
    /// Applies this authorization to a request builder.
    pub(crate) fn apply(self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            ProxyAuth::None => req,
            ProxyAuth::Basic { username, password } => req.basic_auth(username, Some(password)),
            ProxyAuth::Bearer(token) => req.bearer_auth(token),
        }
    }
}

/// Request body for `GET /jobs/manual/queue`.
#[derive(Serialize)]
struct QueueBody<'a> {
    project_name: &'a str,
}

/// Request body for `POST /runs/manual`. `exclude` is omitted when absent;
/// `full_refresh` is serialized as-is (`None` → `null`) so an absent value stays
/// distinct from an explicit `false`.
#[derive(Serialize)]
struct CreateRunBody<'a> {
    select: &'a str,
    project_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exclude: Option<&'a str>,
    full_refresh: Option<bool>,
    turbo: bool,
}

/// Successful `POST /runs/manual` response: `{"run_id": <int>}`.
#[derive(Deserialize)]
struct CreateRunResponse {
    run_id: i64,
}

/// Every proxy error response carries `{"message": "..."}`.
#[derive(Deserialize)]
struct ErrorBody {
    message: String,
}

/// Builds an error from a non-success response, including the proxy's
/// `{"message": ...}` body when present and falling back to just the status.
async fn error_for(context: &str, resp: reqwest::Response) -> Box<dyn std::error::Error> {
    let status = resp.status();
    match resp.json::<ErrorBody>().await {
        Ok(body) => format!("{context} failed with status {status}: {}", body.message).into(),
        Err(_) => format!("{context} failed with status {status}").into(),
    }
}

/// `GET /jobs/manual/queue` — the run queue for the project's manual-build job.
pub(crate) async fn get_runs_queue(
    http: &reqwest::Client,
    base_url: &str,
    auth: ProxyAuth,
    project_name: &str,
) -> Result<RunsQueue, Box<dyn std::error::Error>> {
    let url = format!("{}/jobs/manual/queue", base_url.trim_end_matches('/'));
    let resp = auth
        .apply(http.get(url).json(&QueueBody { project_name }))
        .send()
        .await?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(error_for("get runs queue", resp).await);
    }
    Ok(resp.json().await?)
}

/// `POST /runs/manual` — create a run; returns the new run's id.
#[allow(clippy::too_many_arguments)] // mirrors the `DbtApiClient::create_run` signature
pub(crate) async fn create_run(
    http: &reqwest::Client,
    base_url: &str,
    auth: ProxyAuth,
    project_name: &str,
    select: &str,
    exclude: Option<&str>,
    full_refresh: Option<bool>,
    turbo: bool,
) -> Result<i64, Box<dyn std::error::Error>> {
    let url = format!("{}/runs/manual", base_url.trim_end_matches('/'));
    let body = CreateRunBody {
        select,
        project_name,
        exclude,
        full_refresh,
        turbo,
    };
    let resp = auth.apply(http.post(url).json(&body)).send().await?;
    if resp.status() != reqwest::StatusCode::CREATED {
        return Err(error_for("create run", resp).await);
    }
    let parsed: CreateRunResponse = resp.json().await?;
    Ok(parsed.run_id)
}

/// `GET /runs/:id` — status of a single run. The endpoint is keyed only by id,
/// so the project name is not needed here.
pub(crate) async fn check_run_status(
    http: &reqwest::Client,
    base_url: &str,
    auth: ProxyAuth,
    run_id: &str,
) -> Result<RunStatus, Box<dyn std::error::Error>> {
    let url = format!("{}/runs/{run_id}", base_url.trim_end_matches('/'));
    let resp = auth.apply(http.get(url)).send().await?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(error_for("check run status", resp).await);
    }
    Ok(resp.json().await?)
}

/// `DELETE /runs/:id` — cancel a single run. Keyed only by id.
pub(crate) async fn cancel_run(
    http: &reqwest::Client,
    base_url: &str,
    auth: ProxyAuth,
    run_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{}/runs/{run_id}", base_url.trim_end_matches('/'));
    let resp = auth.apply(http.delete(url)).send().await?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(error_for("cancel run", resp).await);
    }
    Ok(())
}
