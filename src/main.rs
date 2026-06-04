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
        cli::Commands::Jobs { jobs_subcommands } => {
            match jobs_subcommands {
                cli::JobsSubcommands::Run => {
                    println!("Running job...");
                    // Add your job running logic here
                }
                cli::JobsSubcommands::Manual => {
                    println!("Running manual job...");
                    // Add your manual job logic here
                }
            }
        }
        cli::Commands::Runs { runs_subcommands } => {
            match runs_subcommands {
                cli::RunsSubcommands::List => {
                    println!("Listing runs...");
                    // Add your run listing logic here
                }
                cli::RunsSubcommands::Check => {
                    println!("Checking run...");
                    // Add your run checking logic here
                }
                cli::RunsSubcommands::Cancel => {
                    println!("Canceling run...");
                    // Add your run canceling logic here
                }
            }
        }
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
