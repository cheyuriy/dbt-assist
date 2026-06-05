mod api;
mod cli;
mod commands;
mod gcp;
mod models;
mod utils;
mod verbose;

use clap::Parser;
use cli::CLI;

fn main() {
    let cli = CLI::parse();
    verbose::set_verbose(cli.verbose);

    match cli.command {
        cli::Commands::Setup { test_only, scope } => {
            crate::commands::setup(test_only, scope.map(Into::into));
        }
        cli::Commands::Init => {
            crate::commands::init();
        }
        cli::Commands::Manifest {
            scope,
            project_name,
            manifest_dir,
        } => {
            crate::commands::manifest(scope.map(Into::into), project_name, manifest_dir);
        }
        cli::Commands::Jobs { jobs_subcommands } => match jobs_subcommands {
            cli::JobsSubcommands::Run => {
                println!("Running job...");
                // Add your job running logic here
            }
            cli::JobsSubcommands::Manual {
                select,
                exclude,
                full_refresh,
                project_name,
                turbo,
                scope,
                watch,
                logs_always,
                debug_logs,
                save_files,
            } => {
                crate::commands::jobs::manual(
                    select,
                    exclude,
                    full_refresh,
                    project_name,
                    turbo,
                    scope.map(Into::into),
                    watch,
                    logs_always,
                    debug_logs,
                    save_files,
                );
            }
        },
        cli::Commands::Runs { runs_subcommands } => match runs_subcommands {
            cli::RunsSubcommands::Queue {
                scope,
                project_name,
            } => {
                crate::commands::runs::queue(scope.map(Into::into), project_name);
            }
            cli::RunsSubcommands::Check {
                run_id,
                scope,
                project_name,
                logs_always,
                debug_logs,
                save_files,
            } => {
                crate::commands::runs::check(
                    scope.map(Into::into),
                    project_name,
                    run_id,
                    logs_always,
                    debug_logs,
                    save_files,
                );
            }
            cli::RunsSubcommands::Cancel {
                run_id,
                scope,
                project_name,
            } => {
                crate::commands::runs::cancel(scope.map(Into::into), project_name, run_id);
            }
        },
        cli::Commands::Alias { alias_subcommands } => match alias_subcommands {
            cli::AliasSubcommands::List {
                predefined,
                user,
                project,
            } => {
                crate::commands::alias::list(predefined, user, project);
            }
            cli::AliasSubcommands::Add {
                name,
                target,
                select,
                exclude,
                full_refresh,
            } => {
                crate::commands::alias::add(name, target.into(), select, exclude, full_refresh);
            }
            cli::AliasSubcommands::Remove { name, source } => {
                crate::commands::alias::remove(name, source.map(Into::into));
            }
        },
        cli::Commands::Templates {
            templates_subcommands,
        } => match templates_subcommands {
            cli::TemplatesSubcommands::List {
                predefined,
                user,
                project,
            } => {
                crate::commands::templates::list(predefined, user, project);
            }
            cli::TemplatesSubcommands::Docs { name, source } => {
                crate::commands::templates::docs(name, source.map(Into::into));
            }
            cli::TemplatesSubcommands::Build { args } => {
                crate::commands::templates::build(args);
            }
        },
    }
}
