mod api;
mod cli;
mod commands;
mod gcp;
mod models;
mod util;
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
        cli::Commands::Alias { alias_subcommands } => {
            match alias_subcommands {
                cli::AliasSubcommands::List => {
                    println!("Listing aliases...");
                    // Add your alias listing logic here
                }
                cli::AliasSubcommands::Add => {
                    println!("Adding alias...");
                    // Add your alias adding logic here
                }
                cli::AliasSubcommands::Remove => {
                    println!("Removing alias...");
                    // Add your alias removing logic here
                }
            }
        }
        cli::Commands::Templates {
            templates_subcommands,
        } => {
            match templates_subcommands {
                cli::TemplatesSubcommands::List => {
                    println!("Listing templates...");
                    // Add your template listing logic here
                }
                cli::TemplatesSubcommands::Build => {
                    println!("Building template...");
                    // Add your template building logic here
                }
            }
        }
    }
}
