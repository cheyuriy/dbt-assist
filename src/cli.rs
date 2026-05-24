use clap::{Parser, Subcommand};

#[derive(Parser)]
#[allow(clippy::upper_case_acronyms)]
pub struct CLI {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Setup,
    Init,
    Manifest,
    Jobs {
        #[command(subcommand)]
        jobs_subcommands: JobsSubcommands
    },
    Runs {
        #[command(subcommand)]
        runs_subcommands: RunsSubcommands
    },
    Alias {
        #[command(subcommand)]
        alias_subcommands: AliasSubcommands
    },
    Templates {
        #[command(subcommand)]
        templates_subcommands: TemplatesSubcommands
    }
}

#[derive(Subcommand)]
pub enum JobsSubcommands {
    Run,
    Manual
}

#[derive(Subcommand)]
pub enum AliasSubcommands {
    List, 
    Add,
    Remove
}

#[derive(Subcommand)]
pub enum RunsSubcommands {
    List,
    Check,
    Cancel
}

#[derive(Subcommand)]
pub enum TemplatesSubcommands {
    List,
    Build
}