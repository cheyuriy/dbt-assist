use serde::Deserialize;

/// The dbt API's run-queue response: `{"runs": [...]}`.
#[derive(Deserialize, Debug, Clone)]
pub struct RunsQueue {
    pub runs: Vec<Run>,
}

/// A single run in the queue, as returned by the dbt API.
#[derive(Deserialize, Debug, Clone)]
pub struct Run {
    pub id: i64,
    pub status: i64,
    pub run_duration_humanized: String,
    pub queued_duration_humanized: String,
    pub trigger: Trigger,
    pub job: Job,
}

/// What caused a run to start; `cause` is typically the triggering user.
#[derive(Deserialize, Debug, Clone)]
pub struct Trigger {
    pub cause: String,
}

/// The job behind a run; `execute_steps` are the dbt commands it runs.
#[derive(Deserialize, Debug, Clone)]
pub struct Job {
    pub execute_steps: Vec<String>,
}

/// The dbt API's run-status response (for a single run), as returned by
/// `check_run_status`. The boolean flags are mutually informative: a run is
/// either in progress, or complete (and then success/error), or cancelled.
#[derive(Deserialize, Debug, Clone)]
pub struct RunStatus {
    pub in_progress: bool,
    pub is_complete: bool,
    pub is_success: bool,
    pub is_error: bool,
    pub is_cancelled: bool,
    pub duration: String,
    pub run_steps: Vec<RunStep>,
}

/// A single step of a run. `logs`/`debug_logs` are absent until the step has
/// produced them, so both are optional.
#[derive(Deserialize, Debug, Clone)]
pub struct RunStep {
    pub name: String,
    pub index: i64,
    pub status_humanized: String,
    #[serde(default)]
    pub logs: Option<String>,
    #[serde(default)]
    pub debug_logs: Option<String>,
}

impl RunStatus {
    /// Human-readable label derived from the status flags. Checked in order:
    /// cancelled wins, then in-progress (or not-yet-complete), then the
    /// complete states (success, else failed).
    pub fn status_label(&self) -> &'static str {
        if self.is_cancelled {
            "Cancelled"
        } else if self.in_progress || !self.is_complete {
            "In progress"
        } else if self.is_success {
            "Success"
        } else {
            "Failed"
        }
    }

    /// Whether the run finished unsuccessfully (drives whether logs are printed
    /// even without `--logs-always`). A cancelled run is not a failure.
    pub fn is_failed(&self) -> bool {
        self.is_complete && (self.is_error || !self.is_success) && !self.is_cancelled
    }
}

impl Run {
    /// Human-readable label for the numeric `status` code.
    pub fn status_label(&self) -> &'static str {
        match self.status {
            0 => "Queued",
            1 => "Starting",
            2 => "Running",
            _ => "Unknown",
        }
    }

    /// The first execute step (the run's main task), or `""` if there is none.
    pub fn task(&self) -> &str {
        self.job
            .execute_steps
            .first()
            .map(String::as_str)
            .unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with(status: i64) -> Run {
        Run {
            id: 1,
            status,
            run_duration_humanized: String::new(),
            queued_duration_humanized: String::new(),
            trigger: Trigger {
                cause: String::new(),
            },
            job: Job {
                execute_steps: vec![],
            },
        }
    }

    #[test]
    fn status_label_maps_known_codes() {
        assert_eq!(run_with(0).status_label(), "Queued");
        assert_eq!(run_with(1).status_label(), "Starting");
        assert_eq!(run_with(2).status_label(), "Running");
    }

    #[test]
    fn status_label_maps_unknown_codes() {
        assert_eq!(run_with(3).status_label(), "Unknown");
        assert_eq!(run_with(-1).status_label(), "Unknown");
    }

    #[test]
    fn task_returns_first_step_or_empty() {
        let mut run = run_with(2);
        assert_eq!(run.task(), "");
        run.job.execute_steps = vec!["dbt build".to_string(), "dbt test".to_string()];
        assert_eq!(run.task(), "dbt build");
    }

    #[test]
    fn deserializes_api_response() {
        let json = r#"{"runs":[{"id":1234,"status":1,"run_duration_humanized":"1h12m","queued_duration_humanized":"2h3m1s","trigger":{"cause":"x@y.z"},"job":{"execute_steps":["dbt build"]}}]}"#;
        let queue: RunsQueue = serde_json::from_str(json).expect("parse");
        assert_eq!(queue.runs.len(), 1);
        let run = &queue.runs[0];
        assert_eq!(run.id, 1234);
        assert_eq!(run.status_label(), "Starting");
        assert_eq!(run.trigger.cause, "x@y.z");
        assert_eq!(run.task(), "dbt build");
    }

    fn status_with(
        in_progress: bool,
        is_complete: bool,
        is_success: bool,
        is_error: bool,
        is_cancelled: bool,
    ) -> RunStatus {
        RunStatus {
            in_progress,
            is_complete,
            is_success,
            is_error,
            is_cancelled,
            duration: String::new(),
            run_steps: vec![],
        }
    }

    #[test]
    fn run_status_label_covers_each_state() {
        // cancelled wins even if other flags are set
        assert_eq!(
            status_with(false, true, false, true, true).status_label(),
            "Cancelled"
        );
        assert_eq!(
            status_with(true, false, false, false, false).status_label(),
            "In progress"
        );
        // not yet complete, not explicitly in_progress => still in progress
        assert_eq!(
            status_with(false, false, false, false, false).status_label(),
            "In progress"
        );
        assert_eq!(
            status_with(false, true, true, false, false).status_label(),
            "Success"
        );
        assert_eq!(
            status_with(false, true, false, true, false).status_label(),
            "Failed"
        );
    }

    #[test]
    fn run_status_is_failed() {
        assert!(status_with(false, true, false, true, false).is_failed());
        // cancelled is not a failure
        assert!(!status_with(false, true, false, true, true).is_failed());
        // success is not a failure
        assert!(!status_with(false, true, true, false, false).is_failed());
        // still running is not a failure
        assert!(!status_with(true, false, false, false, false).is_failed());
    }

    #[test]
    fn deserializes_run_status_with_optional_logs() {
        let json = r#"{"in_progress":false,"is_complete":true,"is_success":true,"is_error":false,"is_cancelled":false,"duration":"01:10:12","run_steps":[{"name":"x","logs":"abcd","debug_logs":"abcd","status_humanized":"Success","index":1},{"name":"y","status_humanized":"Running","index":2}]}"#;
        let status: RunStatus = serde_json::from_str(json).expect("parse");
        assert!(status.is_complete);
        assert_eq!(status.status_label(), "Success");
        assert_eq!(status.duration, "01:10:12");
        assert_eq!(status.run_steps.len(), 2);

        let first = &status.run_steps[0];
        assert_eq!(first.index, 1);
        assert_eq!(first.name, "x");
        assert_eq!(first.status_humanized, "Success");
        assert_eq!(first.logs.as_deref(), Some("abcd"));
        assert_eq!(first.debug_logs.as_deref(), Some("abcd"));

        // Second step omits both log fields — they default to None.
        let second = &status.run_steps[1];
        assert_eq!(second.index, 2);
        assert!(second.logs.is_none());
        assert!(second.debug_logs.is_none());
    }
}
