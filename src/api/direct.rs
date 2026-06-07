use serde::{Deserialize, Serialize};

use super::client::{DbtApiClient, check_ping_ok};
use crate::models::runs::{Run, RunStatus, RunsQueue};

/// Direct connection to the dbt API: requests go to `url` and are authorized
/// with `token` as a `Bearer` token. Unlike the proxies, this connector
/// orchestrates the raw dbt Cloud Admin API v2 endpoints itself, so it needs
/// the account id, the name of the job dedicated to dbt-assist, the target, and
/// the thread counts to use for normal vs. turbo runs.
pub struct DirectClient {
    http: reqwest::Client,
    url: String,
    token: String,
    account_id: i64,
    dbt_assist_job_name: String,
    dbt_target_name: String,
    username: Option<String>,
    default_threads_num: Option<i64>,
    turbo_threads_num: Option<i64>,
}

impl DirectClient {
    #[allow(clippy::too_many_arguments)] // mirrors the fields carried by the config's Direct variant
    pub fn new(
        http: reqwest::Client,
        url: String,
        token: String,
        account_id: i64,
        dbt_assist_job_name: String,
        dbt_target_name: String,
        username: Option<String>,
        default_threads_num: Option<i64>,
        turbo_threads_num: Option<i64>,
    ) -> Self {
        Self {
            http,
            url,
            token,
            account_id,
            dbt_assist_job_name,
            dbt_target_name,
            username,
            default_threads_num,
            turbo_threads_num,
        }
    }

    /// Base path for account-scoped endpoints: `{url}/v2/accounts/{account_id}`.
    fn account_base(&self) -> String {
        format!(
            "{}/v2/accounts/{}",
            self.url.trim_end_matches('/'),
            self.account_id
        )
    }

    /// Looks up the dbt project id for `project_name`. Names are compared after
    /// lower-casing and turning `-`/` ` into `_`, so e.g. `My Project` matches
    /// `my_project`.
    async fn resolve_project_id(
        &self,
        project_name: &str,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let url = format!("{}/projects/", self.account_base());
        let resp = self.http.get(url).bearer_auth(&self.token).send().await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(error_for("list projects", resp).await);
        }
        let env: Envelope<Vec<ProjectRef>> = resp.json().await?;
        let target = normalize(project_name);
        env.data
            .into_iter()
            .find(|p| normalize(&p.name) == target)
            .map(|p| p.id)
            .ok_or_else(|| format!("project '{project_name}' not found in dbt account").into())
    }

    /// Looks up the dbt-assist job within `project_id`, returning its id and the
    /// environment id it runs in.
    async fn resolve_job(&self, project_id: i64) -> Result<(i64, i64), Box<dyn std::error::Error>> {
        let url = format!("{}/jobs/", self.account_base());
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .query(&[("project_id", project_id.to_string())])
            .send()
            .await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(error_for("list jobs", resp).await);
        }
        let env: Envelope<Vec<JobRef>> = resp.json().await?;
        env.data
            .into_iter()
            .find(|j| j.name == self.dbt_assist_job_name)
            .map(|j| (j.id, j.environment_id))
            .ok_or_else(|| {
                format!(
                    "job '{}' not found in project {project_id}",
                    self.dbt_assist_job_name
                )
                .into()
            })
    }
}

/// Lower-cases `name` and replaces `-`/` ` with `_` so project names can be
/// compared loosely.
fn normalize(name: &str) -> String {
    name.to_lowercase().replace(['-', ' '], "_")
}

/// Builds an error from a non-success dbt API response, appending the response
/// body when present and falling back to just the status.
async fn error_for(context: &str, resp: reqwest::Response) -> Box<dyn std::error::Error> {
    let status = resp.status();
    match resp.text().await {
        Ok(body) if !body.trim().is_empty() => {
            format!("{context} failed with status {status}: {body}").into()
        }
        _ => format!("{context} failed with status {status}").into(),
    }
}

/// The dbt API wraps every payload in `{"data": ...}`.
#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

/// A project, as returned by List Projects (other fields ignored).
#[derive(Deserialize)]
struct ProjectRef {
    id: i64,
    name: String,
}

/// A job, as returned by List Jobs (other fields ignored).
#[derive(Deserialize)]
struct JobRef {
    id: i64,
    name: String,
    environment_id: i64,
}

/// Trigger Job Run returns the freshly created run; we only need its id.
#[derive(Deserialize)]
struct RunIdRef {
    id: i64,
}

/// Cancel Run echoes the run; its numeric `status` tells us whether the cancel
/// took effect.
#[derive(Deserialize)]
struct CancelRef {
    status: i64,
}

/// The `settings` object of an Update Job body.
#[derive(Serialize)]
struct JobSettings<'a> {
    threads: i64,
    target_name: &'a str,
}

/// Minimal Update Job body: just enough to point the dbt-assist job at a fresh
/// `dbt build` step with the right target and thread count.
#[derive(Serialize)]
struct UpdateJobBody<'a> {
    id: i64,
    account_id: i64,
    project_id: i64,
    environment_id: i64,
    name: &'a str,
    settings: JobSettings<'a>,
    execute_steps: Vec<String>,
}

/// Trigger Job Run body.
#[derive(Serialize)]
struct TriggerBody {
    cause: String,
}

impl DbtApiClient for DirectClient {
    async fn ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/v2/accounts", self.url.trim_end_matches('/'));
        let resp = self.http.get(url).bearer_auth(&self.token).send().await?;
        check_ping_ok(resp)
    }

    async fn get_runs_queue(
        &self,
        project_name: &str,
    ) -> Result<RunsQueue, Box<dyn std::error::Error>> {
        let project_id = self.resolve_project_id(project_name).await?;
        let (job_id, _) = self.resolve_job(project_id).await?;

        let url = format!("{}/runs/", self.account_base());
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .query(&[
                ("job_definition_id", job_id.to_string()),
                // dbt expects this as a string, not a JSON array.
                ("include_related", "[trigger, job]".to_string()),
                ("order_by", "created_at".to_string()),
            ])
            .send()
            .await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(error_for("list runs", resp).await);
        }

        let env: Envelope<Vec<Run>> = resp.json().await?;
        // Keep only active/queued runs and remap dbt's status codes
        // (1 Queued, 2 Starting, 3 Running) onto the model's (0, 1, 2).
        let runs = env
            .data
            .into_iter()
            .filter_map(|mut run| match run.status {
                1 => {
                    run.status = 0;
                    Some(run)
                }
                2 => {
                    run.status = 1;
                    Some(run)
                }
                3 => {
                    run.status = 2;
                    Some(run)
                }
                _ => None,
            })
            .collect();
        Ok(RunsQueue { runs })
    }

    async fn create_run(
        &self,
        project_name: &str,
        select: &str,
        exclude: Option<&str>,
        full_refresh: Option<bool>,
        turbo: bool,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let username = self.username.as_deref().unwrap_or("user");
        let project_id = self.resolve_project_id(project_name).await?;
        let (job_id, environment_id) = self.resolve_job(project_id).await?;

        let mut command = format!("dbt build --select {select}");
        if let Some(exclude) = exclude {
            command.push_str(&format!(" --exclude {exclude}"));
        }
        if full_refresh == Some(true) {
            command.push_str(" --full-refresh");
        }

        let threads = if turbo {
            self.turbo_threads_num.unwrap_or(4)
        } else {
            self.default_threads_num.unwrap_or(1)
        };

        // Point the job at our freshly built command, then trigger it.
        let update_url = format!("{}/jobs/{job_id}/", self.account_base());
        let update_body = UpdateJobBody {
            id: job_id,
            account_id: self.account_id,
            project_id,
            environment_id,
            name: &self.dbt_assist_job_name,
            settings: JobSettings {
                threads,
                target_name: &self.dbt_target_name,
            },
            execute_steps: vec![command],
        };
        let resp = self
            .http
            .post(update_url)
            .bearer_auth(&self.token)
            .json(&update_body)
            .send()
            .await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(error_for("update job", resp).await);
        }

        let trigger_url = format!("{}/jobs/{job_id}/run/", self.account_base());
        let trigger_body = TriggerBody {
            cause: format!("{username} via dbt-assist"),
        };
        let resp = self
            .http
            .post(trigger_url)
            .bearer_auth(&self.token)
            .json(&trigger_body)
            .send()
            .await?;
        if resp.status() != reqwest::StatusCode::OK && resp.status() != reqwest::StatusCode::CREATED
        {
            return Err(error_for("trigger job run", resp).await);
        }
        let env: Envelope<RunIdRef> = resp.json().await?;
        Ok(env.data.id)
    }

    async fn check_run_status(
        &self,
        _project_name: &str,
        run_id: &str,
    ) -> Result<RunStatus, Box<dyn std::error::Error>> {
        let url = format!("{}/runs/{run_id}/", self.account_base());
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .query(&[
                ("include_related", "debug_logs"),
                ("include_related", "run_steps"),
            ])
            .send()
            .await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(error_for("check run status", resp).await);
        }
        let env: Envelope<RunStatus> = resp.json().await?;
        Ok(env.data)
    }

    async fn cancel_run(
        &self,
        _project_name: &str,
        run_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/runs/{run_id}/cancel/", self.account_base());
        let resp = self.http.post(url).bearer_auth(&self.token).send().await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(error_for("cancel run", resp).await);
        }
        let env: Envelope<CancelRef> = resp.json().await?;
        // A run that is queued/starting/running (1/2/3) or cancelled (30) is
        // considered successfully cancelled.
        if matches!(env.data.status, 1 | 2 | 3 | 30) {
            Ok(())
        } else {
            Err(format!(
                "cancel run did not take effect; run status is {}",
                env.data.status
            )
            .into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::{
        Method::{GET, POST},
        MockServer,
    };
    use serde_json::json;

    fn client(server: &MockServer, token: &str) -> DirectClient {
        DirectClient::new(
            reqwest::Client::new(),
            server.base_url(),
            token.to_string(),
            42,
            "dbt-assist".to_string(),
            "prod".to_string(),
            None,
            None,
            None,
        )
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

    /// Mocks List Projects returning a single project whose name matches loosely.
    async fn mock_projects(server: &MockServer) {
        server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/accounts/42/projects/");
                then.status(200)
                    .json_body(json!({"data": [{"id": 7, "name": "My Project"}]}));
            })
            .await;
    }

    /// Mocks List Jobs returning the dbt-assist job.
    async fn mock_jobs(server: &MockServer) {
        server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/accounts/42/jobs/");
                then.status(200).json_body(json!({"data": [
                    {"id": 99, "name": "other", "environment_id": 1},
                    {"id": 100, "name": "dbt-assist", "environment_id": 5}
                ]}));
            })
            .await;
    }

    #[tokio::test]
    async fn create_run_resolves_updates_and_triggers() {
        let server = MockServer::start_async().await;
        mock_projects(&server).await;
        mock_jobs(&server).await;
        // Update Job must receive the dbt build command, prod target, and 1 thread.
        let update = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v2/accounts/42/jobs/100/")
                    .body_contains(
                        "dbt build --select tag:nightly --exclude model_x --full-refresh",
                    )
                    .body_contains("\"target_name\":\"prod\"")
                    .body_contains("\"threads\":1");
                then.status(200).json_body(json!({"data": {"id": 100}}));
            })
            .await;
        // Trigger Job Run returns the new run id and must carry the cause.
        let trigger = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v2/accounts/42/jobs/100/run/")
                    .body_contains("user via dbt-assist");
                then.status(200).json_body(json!({"data": {"id": 555}}));
            })
            .await;

        let run_id = client(&server, "t")
            .create_run(
                "my-project",
                "tag:nightly",
                Some("model_x"),
                Some(true),
                false,
            )
            .await
            .expect("create run");

        assert_eq!(run_id, 555);
        update.assert_async().await;
        trigger.assert_async().await;
    }

    #[tokio::test]
    async fn create_run_uses_turbo_threads() {
        let server = MockServer::start_async().await;
        mock_projects(&server).await;
        mock_jobs(&server).await;
        let update = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v2/accounts/42/jobs/100/")
                    .body_contains("\"threads\":4");
                then.status(200).json_body(json!({"data": {"id": 100}}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/v2/accounts/42/jobs/100/run/");
                then.status(200).json_body(json!({"data": {"id": 1}}));
            })
            .await;

        let mut c = client(&server, "t");
        c.turbo_threads_num = Some(4);
        c.create_run("my-project", "*", None, None, true)
            .await
            .expect("create run");

        update.assert_async().await;
    }

    #[tokio::test]
    async fn create_run_errors_on_unknown_project() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/accounts/42/projects/");
                then.status(200)
                    .json_body(json!({"data": [{"id": 7, "name": "something else"}]}));
            })
            .await;

        let result = client(&server, "t")
            .create_run("my-project", "*", None, None, false)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_run_status_unwraps_data() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/accounts/42/runs/777/");
                then.status(200).json_body(json!({"data": {
                    "in_progress": false,
                    "is_complete": true,
                    "is_success": true,
                    "is_error": false,
                    "is_cancelled": false,
                    "duration": "00:01:23",
                    "run_steps": [
                        {"name": "build", "index": 1, "status_humanized": "Success", "logs": "ok"}
                    ]
                }}));
            })
            .await;

        let status = client(&server, "t")
            .check_run_status("my-project", "777")
            .await
            .expect("check status");

        assert!(status.is_success);
        assert_eq!(status.status_label(), "Success");
        assert_eq!(status.run_steps.len(), 1);
        assert_eq!(status.run_steps[0].logs.as_deref(), Some("ok"));
    }

    #[tokio::test]
    async fn cancel_run_accepts_cancellable_status() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/v2/accounts/42/runs/777/cancel/");
                then.status(200).json_body(json!({"data": {"status": 30}}));
            })
            .await;

        let result = client(&server, "t").cancel_run("my-project", "777").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cancel_run_rejects_terminal_status() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/v2/accounts/42/runs/777/cancel/");
                then.status(200).json_body(json!({"data": {"status": 10}}));
            })
            .await;

        let result = client(&server, "t").cancel_run("my-project", "777").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_runs_queue_filters_and_remaps_status() {
        let server = MockServer::start_async().await;
        mock_projects(&server).await;
        mock_jobs(&server).await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/v2/accounts/42/runs/")
                    .query_param("job_definition_id", "100");
                then.status(200).json_body(json!({"data": [
                    {"id": 1, "status": 1, "run_duration_humanized": "", "queued_duration_humanized": "1m", "trigger": {"cause": "a"}, "job": {"execute_steps": ["dbt build"]}},
                    {"id": 2, "status": 3, "run_duration_humanized": "2m", "queued_duration_humanized": "", "trigger": {"cause": "b"}, "job": {"execute_steps": ["dbt build"]}},
                    {"id": 3, "status": 10, "run_duration_humanized": "5m", "queued_duration_humanized": "", "trigger": {"cause": "c"}, "job": {"execute_steps": ["dbt build"]}}
                ]}));
            })
            .await;

        let queue = client(&server, "t")
            .get_runs_queue("my-project")
            .await
            .expect("queue");

        // The completed run (status 10) is filtered out; the others are remapped.
        assert_eq!(queue.runs.len(), 2);
        assert_eq!(queue.runs[0].id, 1);
        assert_eq!(queue.runs[0].status_label(), "Queued"); // 1 -> 0
        assert_eq!(queue.runs[1].id, 2);
        assert_eq!(queue.runs[1].status_label(), "Running"); // 3 -> 2
    }
}
