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
}
