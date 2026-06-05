use std::io::Write;
use std::time::{Duration, Instant};

use colored::Colorize;

use crate::api::client::DbtApiClient;
use crate::commands::runs;
use crate::models::config::ConfigScope;
use crate::models::runs::RunStatus;

/// Watch-loop tuning. The loop stops at whichever limit is hit first.
const MAX_ITERATIONS: u32 = 60; // hard cap on poll iterations
const LATENCY_SECS: u64 = 3; // delay between polls
const MAX_TIME_SECS: u64 = 300; // overall time budget
const EXTRA_ITERATIONS: u32 = 2; // extra polls after a terminal status (lets logs populate)

/// `jobs manual`: create a one-off run that builds the selected models on the
/// production job, then (with `--watch`) poll it to completion.
#[allow(clippy::too_many_arguments)]
pub fn manual(
    select: String,
    exclude: Option<String>,
    full_refresh: Option<bool>,
    project_name: Option<String>,
    turbo: bool,
    scope: Option<ConfigScope>,
    watch: bool,
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

    // This command drives a build of *this* dbt project, so it must run from a
    // project root — even when --project-name overrides the name.
    if !crate::utils::is_dbt_project(&cwd) {
        eprintln!(
            "{} run inside a dbt project directory (no dbt_project.yml found here)",
            "error:".red().bold()
        );
        return;
    }

    let (project, api) = match runs::prepare(scope, project_name.clone()) {
        Ok(prepared) => prepared,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            return;
        }
    };

    // Step 2: create the run and get its id. (Step 1 — estimating build impact
    // and checking the queue — is intentionally not implemented yet.)
    let run_id = match runs::block_on(async {
        api.create_run(&project, &select, exclude.as_deref(), full_refresh, turbo)
            .await
    }) {
        Ok(Ok(run_id)) => run_id,
        Ok(Err(e)) => {
            eprintln!("{} Could not create run: {e}", "error:".red().bold());
            return;
        }
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            return;
        }
    };
    // The status/cancel APIs take the id as a string (it arrives that way from
    // the CLI elsewhere); convert once here.
    let run_id = run_id.to_string();

    if !watch {
        println!("{}", format!("Run created: {run_id}").green());
        println!(
            "Check status with: {} runs check {run_id}",
            env!("CARGO_PKG_NAME")
        );
        return;
    }

    // Step 3: poll the run, redrawing the status table in place each iteration.
    let mut redrawer = Redrawer::default();
    let final_status = match watch_run(scope, project_name, &run_id, &mut redrawer) {
        Some(status) => status,
        None => return,
    };

    let logs_dir = if save_files {
        match runs::save_logs(&cwd, &run_id, &final_status) {
            Ok(dir) => Some(dir),
            Err(e) => {
                eprintln!("{} Could not save logs: {e}", "warning:".yellow().bold());
                None
            }
        }
    } else {
        None
    };

    // Final frame: redraw over the last live frame so the outcome (now with the
    // logs directory) replaces it rather than stacking a duplicate table.
    redrawer.draw(&runs::build_status_table(&final_status, logs_dir.as_deref()).to_string());

    if logs_always || final_status.is_failed() {
        runs::print_logs(&final_status, debug_logs);
    }
}

/// Poll `run_id` until it reaches a terminal status (plus a few extra polls to
/// let logs populate) or a loop limit is hit, redrawing the status table in
/// place each iteration. Returns the last fetched status, or `None` if a fetch
/// failed (the error is already reported).
fn watch_run(
    scope: Option<ConfigScope>,
    project_name: Option<String>,
    run_id: &str,
    redrawer: &mut Redrawer,
) -> Option<RunStatus> {
    let start = Instant::now();
    let mut iteration: u32 = 0;
    // `Some(n)` once a terminal status has been seen: n extra polls remain.
    let mut extra_left: Option<u32> = None;
    let mut final_status;

    loop {
        let status = match runs::fetch_status(scope, project_name.clone(), run_id) {
            Ok(status) => status,
            Err(e) => {
                eprintln!("{} {e}", "error:".red().bold());
                return None;
            }
        };

        redrawer.draw(&runs::build_status_table(&status, None).to_string());

        let terminal = is_terminal(&status);
        final_status = status;

        match extra_left {
            Some(0) => break,
            Some(n) => extra_left = Some(n - 1),
            None if terminal => extra_left = Some(EXTRA_ITERATIONS),
            None => {}
        }

        iteration += 1;
        if iteration >= MAX_ITERATIONS || start.elapsed().as_secs() >= MAX_TIME_SECS {
            break;
        }

        std::thread::sleep(Duration::from_secs(LATENCY_SECS));
    }

    Some(final_status)
}

/// Whether the run has reached a terminal status (success, error, or cancelled).
fn is_terminal(status: &RunStatus) -> bool {
    status.is_cancelled || status.is_failed() || (status.is_complete && status.is_success)
}

/// Redraws a block of text in place: before each frame it moves the cursor back
/// up over the previously drawn block and clears from there to the end of the
/// screen, so successive frames overwrite the last instead of stacking. Unlike a
/// full-screen clear (`\x1B[2J`), this doesn't push old content into scrollback,
/// so it behaves the same across terminals (including VS Code's).
#[derive(Default)]
struct Redrawer {
    /// Number of terminal lines the previously drawn block occupied.
    prev_lines: u16,
}

impl Redrawer {
    fn draw(&mut self, block: &str) {
        let mut out = std::io::stdout();
        if self.prev_lines > 0 {
            // Move up over the previous block, then clear to end of screen.
            let _ = write!(out, "\x1B[{}A\x1B[0J", self.prev_lines);
        }
        let _ = writeln!(out, "{block}");
        let _ = out.flush();
        self.prev_lines = block.lines().count() as u16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::runs::RunStatus;

    fn status(
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
            duration: "00:00:10".to_string(),
            run_steps: Vec::new(),
        }
    }

    #[test]
    fn is_terminal_true_for_success() {
        // complete + success
        assert!(is_terminal(&status(false, true, true, false, false)));
    }

    #[test]
    fn is_terminal_true_for_error() {
        // complete + error (is_failed())
        assert!(is_terminal(&status(false, true, false, true, false)));
    }

    #[test]
    fn is_terminal_true_for_cancelled() {
        assert!(is_terminal(&status(false, false, false, false, true)));
    }

    #[test]
    fn is_terminal_false_while_running() {
        assert!(!is_terminal(&status(true, false, false, false, false)));
    }

    #[test]
    fn is_terminal_false_when_not_yet_complete() {
        assert!(!is_terminal(&status(false, false, false, false, false)));
    }
}
