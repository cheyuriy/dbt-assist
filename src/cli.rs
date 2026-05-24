use clap::{Parser, Subcommand};

/// Tool to assist your work with DBT
#[derive(Parser)]
#[command(version, about = "A CLI tool to assist your work with dbt", long_about = None, disable_help_subcommand = true)]
#[allow(clippy::upper_case_acronyms)]
pub struct CLI {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Set up all necessary settings and directories to make dbt-assist work
    Setup,

    /// Initialize the current dbt project to work with VSCode and dbt-assist
    Init,

    /// Refresh local manifest.json in current project to allow using `defer` in dbt
    Manifest,

    /// Run jobs in the production environment
    Jobs {
        #[command(subcommand)]
        jobs_subcommands: JobsSubcommands,
    },

    /// Manage and check results of jobs' runs
    Runs {
        #[command(subcommand)]
        runs_subcommands: RunsSubcommands,
    },

    /// Manage aliases for jobs
    Alias {
        #[command(subcommand)]
        alias_subcommands: AliasSubcommands,
    },

    /// Use templates to create predefined models
    Templates {
        #[command(subcommand)]
        templates_subcommands: TemplatesSubcommands,
    },
}

#[derive(Subcommand)]
pub enum JobsSubcommands {
    /// Run an existing alias in the production environment
    Run,

    /// Run a one-time job to build models specified by a query in the production environment
    Manual,
}

#[derive(Subcommand)]
pub enum AliasSubcommands {
    /// List all configured aliases
    List,

    /// Add a new alias
    Add,

    /// Remove an existing alias
    Remove,
}

#[derive(Subcommand)]
pub enum RunsSubcommands {
    /// List all active and queued runs
    List,

    /// Check the status and show logs (if any) for a specific run
    Check,

    /// Cancel a specific run by ID (running or queued)
    Cancel,
}

#[derive(Subcommand)]
pub enum TemplatesSubcommands {
    /// List available templates
    List,

    /// Use a template to create a new model
    Build,
}
