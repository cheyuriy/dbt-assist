use std::path::{Path, PathBuf};

use colored::Colorize;
use comfy_table::{Cell, Table, presets::UTF8_FULL};

use crate::api::client::{DbtApi, DbtApiClient};
use crate::models::config::{AppConfig, ConfigScope, load_config};
use crate::models::runs::RunStatus;
use crate::vprintln;

/// `runs queue`: fetch the run queue for the project and print it as a table.
pub fn queue(scope: Option<ConfigScope>, project_name: Option<String>) {
    let (project, api) = match prepare(scope, project_name) {
        Ok(prepared) => prepared,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            return;
        }
    };

    let result = match block_on(async { api.get_runs_queue(&project).await }) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            return;
        }
    };

    let queue = match result {
        Ok(queue) => queue,
        Err(e) => {
            eprintln!("{} could not fetch runs: {e}", "error:".red().bold());
            return;
        }
    };

    if queue.runs.is_empty() {
        println!("{}", "No active or queued runs.".dimmed());
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL).set_header(vec![
        Cell::new("Run ID"),
        Cell::new("Status"),
        Cell::new("In run"),
        Cell::new("In queue"),
        Cell::new("Run by"),
        Cell::new("Task"),
    ]);

    for run in &queue.runs {
        table.add_row(vec![
            Cell::new(run.id.to_string()),
            Cell::new(run.status_label()),
            Cell::new(&run.run_duration_humanized),
            Cell::new(&run.queued_duration_humanized),
            Cell::new(&run.trigger.cause),
            Cell::new(run.task()),
        ]);
    }

    println!("{table}");
}

/// `runs check`: fetch the status of `run_id`, print it as a table, and
/// optionally print and/or save the per-step logs.
pub fn check(
    scope: Option<ConfigScope>,
    project_name: Option<String>,
    run_id: String,
    logs_always: bool,
    debug_logs: bool,
    save_files: bool,
) {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            return;
        }
    };

    if save_files && !crate::utils::is_dbt_project(&cwd) {
        eprintln!(
            "{} {} writes to {}, so run inside a dbt project directory (no {} found here).",
            "error:".red().bold(),
            "--save-files".bold(),
            ".logs/".bold(),
            "dbt_project.yml".bold()
        );
        return;
    }

    let status = match fetch_status(scope, project_name, &run_id) {
        Ok(status) => status,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            return;
        }
    };

    let logs_dir = if save_files {
        match save_logs(&cwd, &run_id, &status) {
            Ok(dir) => Some(dir),
            Err(e) => {
                eprintln!("{} could not save logs: {e}", "warning:".yellow().bold());
                None
            }
        }
    } else {
        None
    };

    println!("{}", build_status_table(&status, logs_dir.as_deref()));

    if logs_always || status.is_failed() {
        print_logs(&status, debug_logs);
    }
}

/// `runs cancel`: cancel the run `run_id` within the project and confirm.
pub fn cancel(scope: Option<ConfigScope>, project_name: Option<String>, run_id: String) {
    let (project, api) = match prepare(scope, project_name) {
        Ok(prepared) => prepared,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            return;
        }
    };

    let result = match block_on(async { api.cancel_run(&project, &run_id).await }) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            return;
        }
    };

    match result {
        Ok(()) => println!("{} Run {} cancelled.", "✓".green().bold(), run_id.bold()),
        Err(e) => eprintln!("{} could not cancel run: {e}", "error:".red().bold()),
    }
}

/// Shared setup for the runs subcommands: resolve cwd + project name, load the
/// config for `scope`, and build the API client. Returns the project name and a
/// ready client.
pub(crate) fn prepare(
    scope: Option<ConfigScope>,
    project_name: Option<String>,
) -> Result<(String, DbtApi), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;
    let project = resolve_project_name(project_name, &cwd)?;

    let (config, resolved): (AppConfig, ConfigScope) = load_config(scope)?;
    vprintln!("Loaded {resolved} config");

    let api = DbtApi::from_config(&config)?;
    Ok((project, api))
}

/// Build and run a current-thread tokio runtime for the async body.
pub(crate) fn block_on<F: std::future::Future>(
    fut: F,
) -> Result<F::Output, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(rt.block_on(fut))
}

/// Fetch the status of a single run. Factored out of [`check`] so a future
/// "watch in a loop" command can reuse it: it returns the typed [`RunStatus`]
/// and prints nothing. Unwraps the runtime layer then the API layer (like
/// [`queue`]).
pub(crate) fn fetch_status(
    scope: Option<ConfigScope>,
    project_name: Option<String>,
    run_id: &str,
) -> Result<RunStatus, Box<dyn std::error::Error>> {
    let (project, api) = prepare(scope, project_name)?;
    let status = block_on(async { api.check_run_status(&project, run_id).await })??;
    Ok(status)
}

/// Build the run-status table. Glyphs are left uncolored so `comfy_table`
/// measures column widths correctly. `logs_dir` is the directory logs were
/// saved to (when `--save-files` was used).
pub(crate) fn build_status_table(status: &RunStatus, logs_dir: Option<&Path>) -> Table {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL).set_header(vec![
        Cell::new("Run status"),
        Cell::new("Duration"),
        Cell::new("Steps"),
        Cell::new("Logs"),
        Cell::new("Debug logs"),
        Cell::new("Logs directory"),
    ]);

    let steps = step_icons(status, |s| step_status_icon(&s.status_humanized));
    let logs = step_icons(status, |s| presence_icon(s.logs.is_some()));
    let debug = step_icons(status, |s| presence_icon(s.debug_logs.is_some()));
    let dir = logs_dir
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    table.add_row(vec![
        Cell::new(status.status_label()),
        Cell::new(&status.duration),
        Cell::new(steps),
        Cell::new(logs),
        Cell::new(debug),
        Cell::new(dir),
    ]);

    table
}

/// Save each step's logs to `.logs/<run_id>/`. Writes `logs_<index>_<name>.log`
/// for normal logs and `debug_<index>_<name>.log` for debug logs, but only for
/// steps that actually carry that payload. Returns the created directory.
pub(crate) fn save_logs(
    cwd: &Path,
    run_id: &str,
    status: &RunStatus,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = cwd.join(".logs").join(run_id);
    std::fs::create_dir_all(&dir)?;

    for step in &status.run_steps {
        let name = sanitize_for_filename(&step.name);
        if let Some(logs) = &step.logs {
            std::fs::write(dir.join(format!("logs_{}_{}.log", step.index, name)), logs)?;
        }
        if let Some(debug_logs) = &step.debug_logs {
            std::fs::write(
                dir.join(format!("debug_{}_{}.log", step.index, name)),
                debug_logs,
            )?;
        }
    }

    Ok(dir)
}

/// Print the logs for every step after the table: a bold divider with the
/// step's index and name, then the chosen log type (or a dimmed placeholder
/// when that step has no such logs yet).
pub(crate) fn print_logs(status: &RunStatus, debug: bool) {
    for step in &status.run_steps {
        println!(
            "\n{}",
            format!("──── [{}] {} ────", step.index, step.name).bold()
        );
        let content = if debug { &step.debug_logs } else { &step.logs };
        match content {
            Some(text) => println!("{text}"),
            None => println!("{}", "(no logs)".dimmed()),
        }
    }
}

/// Resolve the dbt project name: the `--project-name` override wins; otherwise
/// read `name:` from `dbt_project.yml`, which requires running inside a dbt
/// project.
fn resolve_project_name(
    override_: Option<String>,
    cwd: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(name) = override_ {
        return Ok(name);
    }
    if !crate::utils::is_dbt_project(cwd) {
        return Err(format!(
            "run inside a dbt project directory (no {} found here) or pass {}",
            "dbt_project.yml".bold(),
            "--project-name".bold()
        )
        .into());
    }
    crate::utils::read_project_name(cwd).ok_or_else(|| {
        format!(
            "could not read `name:` from {}; pass {}",
            "dbt_project.yml".bold(),
            "--project-name".bold()
        )
        .into()
    })
}

/// Glyph reflecting a step's `status_humanized`.
fn step_status_icon(status_humanized: &str) -> &'static str {
    match status_humanized {
        "Success" => "✓",
        "Running" => "●",
        "Error" => "✗",
        _ => "?",
    }
}

/// Glyph reflecting whether a log payload is present for a step.
fn presence_icon(present: bool) -> &'static str {
    if present { "✓" } else { "–" }
}

/// Join one glyph per step, space-separated, via `pick`.
fn step_icons(
    status: &RunStatus,
    pick: impl Fn(&crate::models::runs::RunStep) -> &'static str,
) -> String {
    status
        .run_steps
        .iter()
        .map(&pick)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Replace characters that are awkward in filenames (path separators and
/// whitespace) with underscores.
fn sanitize_for_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_whitespace() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::runs::RunStep;

    fn step(index: i64, name: &str, logs: Option<&str>, debug_logs: Option<&str>) -> RunStep {
        RunStep {
            name: name.to_string(),
            index,
            status_humanized: "Success".to_string(),
            logs: logs.map(str::to_string),
            debug_logs: debug_logs.map(str::to_string),
        }
    }

    fn status_with(run_steps: Vec<RunStep>) -> RunStatus {
        RunStatus {
            in_progress: false,
            is_complete: true,
            is_success: true,
            is_error: false,
            is_cancelled: false,
            duration: "00:01:00".to_string(),
            run_steps,
        }
    }

    #[test]
    fn step_status_icon_maps_statuses() {
        assert_eq!(step_status_icon("Success"), "✓");
        assert_eq!(step_status_icon("Running"), "●");
        assert_eq!(step_status_icon("Error"), "✗");
        assert_eq!(step_status_icon("Whatever"), "?");
    }

    #[test]
    fn presence_icon_reflects_presence() {
        assert_eq!(presence_icon(true), "✓");
        assert_eq!(presence_icon(false), "–");
    }

    #[test]
    fn sanitize_for_filename_replaces_separators_and_whitespace() {
        assert_eq!(sanitize_for_filename("dbt build"), "dbt_build");
        assert_eq!(sanitize_for_filename("a/b\\c"), "a_b_c");
        assert_eq!(sanitize_for_filename("plain"), "plain");
    }

    #[test]
    fn save_logs_writes_only_present_payloads_with_sanitized_names() {
        let tmp = tempfile::tempdir().unwrap();
        let status = status_with(vec![
            step(1, "dbt build", Some("normal-1"), Some("debug-1")),
            step(2, "dbt test", Some("normal-2"), None),
            step(3, "no logs yet", None, None),
        ]);

        let dir = save_logs(tmp.path(), "456", &status).expect("save logs");
        assert_eq!(dir, tmp.path().join(".logs").join("456"));

        // Step 1: both files, name sanitized.
        assert_eq!(
            std::fs::read_to_string(dir.join("logs_1_dbt_build.log")).unwrap(),
            "normal-1"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("debug_1_dbt_build.log")).unwrap(),
            "debug-1"
        );
        // Step 2: normal logs only.
        assert_eq!(
            std::fs::read_to_string(dir.join("logs_2_dbt_test.log")).unwrap(),
            "normal-2"
        );
        assert!(!dir.join("debug_2_dbt_test.log").exists());
        // Step 3: nothing written.
        assert!(!dir.join("logs_3_no_logs_yet.log").exists());
        assert!(!dir.join("debug_3_no_logs_yet.log").exists());
    }

    #[test]
    fn build_status_table_renders_one_glyph_per_step() {
        let status = status_with(vec![
            step(1, "a", Some("x"), None),
            step(2, "b", None, Some("y")),
        ]);
        let rendered = build_status_table(&status, None).to_string();
        // status_humanized defaults to "Success" for both steps in the helper.
        assert!(rendered.contains("✓ ✓"));
        // logs present only on step 1, debug only on step 2.
        assert!(rendered.contains("✓ –"));
        assert!(rendered.contains("– ✓"));
        assert!(rendered.contains("Success"));
    }
}
