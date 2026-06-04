use std::path::Path;

use colored::Colorize;
use comfy_table::{Cell, Table, presets::UTF8_FULL};

use crate::api::client::{DbtApi, DbtApiClient};
use crate::models::config::{AppConfig, ConfigScope, load_config};
use crate::vprintln;

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
        return Err(
            "run inside a dbt project directory (no dbt_project.yml found here) or pass \
             --project-name"
                .into(),
        );
    }
    crate::utils::read_project_name(cwd)
        .ok_or_else(|| "could not read `name:` from dbt_project.yml; pass --project-name".into())
}

/// Shared setup for the runs subcommands: resolve cwd + project name, load the
/// config for `scope`, and build the API client. Returns the project name and a
/// ready client.
fn prepare(
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
fn block_on<F: std::future::Future>(fut: F) -> Result<F::Output, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(rt.block_on(fut))
}

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
            eprintln!("{} Could not fetch runs: {e}", "error:".red().bold());
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
        Ok(()) => println!("{}", format!("Run {run_id} cancelled.").green()),
        Err(e) => eprintln!("{} Could not cancel run: {e}", "error:".red().bold()),
    }
}
