use clap::{Parser, Subcommand, ValueEnum};

use crate::models::alias::AliasSource;
use crate::models::config::ConfigScope;
use crate::models::template::TemplateSource;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ScopeArg {
    Local,
    Global,
}

impl From<ScopeArg> for ConfigScope {
    fn from(value: ScopeArg) -> Self {
        match value {
            ScopeArg::Local => ConfigScope::Local,
            ScopeArg::Global => ConfigScope::Global,
        }
    }
}

/// Writable alias targets exposed on the CLI (predefined aliases are bundled
/// and cannot be written).
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum AliasTarget {
    User,
    Project,
}

impl From<AliasTarget> for AliasSource {
    fn from(value: AliasTarget) -> Self {
        match value {
            AliasTarget::User => AliasSource::User,
            AliasTarget::Project => AliasSource::Project,
        }
    }
}

/// Template sources selectable on the CLI (all three; predefined templates are
/// readable but bundled and immutable).
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum TemplateSourceArg {
    Predefined,
    User,
    Project,
}

impl From<TemplateSourceArg> for TemplateSource {
    fn from(value: TemplateSourceArg) -> Self {
        match value {
            TemplateSourceArg::Predefined => TemplateSource::Predefined,
            TemplateSourceArg::User => TemplateSource::User,
            TemplateSourceArg::Project => TemplateSource::Project,
        }
    }
}

/// Tool to assist your work with DBT
#[derive(Parser)]
#[command(version, about = "A CLI tool to assist your work with dbt", long_about = None, disable_help_subcommand = true)]
#[allow(clippy::upper_case_acronyms)]
pub struct CLI {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose logging
    #[arg(long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Set up all necessary settings and directories to make dbt-assist work
    Setup {
        /// Only test connectivity and permissions without making any actual changes to the environment
        #[arg(long)]
        test_only: bool,

        /// Target config scope: "local" (./.dbt-assist/) or "global" (user config directory).
        /// Omit to auto-detect; you'll be prompted only if no config exists yet.
        #[arg(long, value_enum)]
        scope: Option<ScopeArg>,
    },

    /// Initialize the current dbt project to work with VSCode and dbt-assist
    Init,

    /// Refresh local manifest.json in current project to allow using `defer` in dbt
    Manifest {
        /// Config scope: "local" (./.dbt-assist/) or "global". Omit to auto-detect.
        #[arg(long, value_enum)]
        scope: Option<ScopeArg>,

        /// Override the dbt project name (defaults to `name:` in dbt_project.yml).
        #[arg(long)]
        project_name: Option<String>,

        /// Directory to store manifest.json (default: .manifest).
        #[arg(long)]
        manifest_dir: Option<String>,
    },

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
    Manual {
        /// dbt selector for models to build (passed to `dbt build`).
        select: String,

        /// dbt selector for models to exclude (passed to `dbt build`).
        #[arg(long)]
        exclude: Option<String>,

        /// Pass --full-refresh to `dbt build` (an absent value is not the same
        /// as false).
        #[arg(long)]
        full_refresh: Option<bool>,

        /// Override the dbt project name (defaults to `name:` in dbt_project.yml).
        #[arg(long)]
        project_name: Option<String>,

        /// Run the build with more threads.
        #[arg(long)]
        turbo: bool,

        /// Config scope: "local" (./.dbt-assist/) or "global". Omit to auto-detect.
        #[arg(long, value_enum)]
        scope: Option<ScopeArg>,

        /// Poll the run to completion, refreshing a live status table.
        #[arg(long)]
        watch: bool,

        /// (with --watch) Always print logs at the end, not only on failure.
        #[arg(long)]
        logs_always: bool,

        /// (with --watch) Print debug logs instead of normal logs.
        #[arg(long)]
        debug_logs: bool,

        /// (with --watch) Save logs (normal and debug) to .logs/<run_id>/.
        #[arg(long)]
        save_files: bool,
    },
}

#[derive(Subcommand)]
pub enum AliasSubcommands {
    /// List all configured aliases. With no flag, shows predefined, user, and
    /// project aliases; pass one or more flags to narrow the list.
    List {
        /// Show bundled, predefined aliases.
        #[arg(long)]
        predefined: bool,

        /// Show user aliases (global config directory).
        #[arg(long)]
        user: bool,

        /// Show project aliases (./.aliases/).
        #[arg(long)]
        project: bool,
    },

    /// Add a new user or project alias
    Add {
        /// Name of the alias (becomes the YAML filename).
        name: String,

        /// Where to store the alias: "user" or "project" (default: project).
        #[arg(long, value_enum, default_value_t = AliasTarget::Project)]
        target: AliasTarget,

        /// Value stored under the `select` key (default: "*", i.e. build all).
        #[arg(long, default_value = "*")]
        select: String,

        /// Value stored under the `exclude` key (omitted when unset).
        #[arg(long)]
        exclude: Option<String>,

        /// Value stored under the `full_refresh` key (omitted when unset; an
        /// absent value is not the same as `false`).
        #[arg(long)]
        full_refresh: Option<bool>,
    },

    /// Remove an existing user or project alias
    Remove {
        /// Name of the alias to remove.
        name: String,

        /// Source to remove from: "user" or "project". Required when the name
        /// exists in more than one source.
        #[arg(long, value_enum)]
        source: Option<AliasTarget>,
    },
}

#[derive(Subcommand)]
pub enum RunsSubcommands {
    /// List active and queued runs for a project
    Queue {
        /// Config scope: "local" (./.dbt-assist/) or "global". Omit to auto-detect.
        #[arg(long, value_enum)]
        scope: Option<ScopeArg>,

        /// Project to query (defaults to `name:` in dbt_project.yml; required
        /// when not run inside a dbt project).
        #[arg(long)]
        project_name: Option<String>,
    },

    /// Check the status and show logs (if any) for a specific run
    Check {
        /// ID of the run to check.
        run_id: String,

        /// Config scope: "local" (./.dbt-assist/) or "global". Omit to auto-detect.
        #[arg(long, value_enum)]
        scope: Option<ScopeArg>,

        /// Project the run belongs to (defaults to `name:` in dbt_project.yml;
        /// required when not run inside a dbt project).
        #[arg(long)]
        project_name: Option<String>,

        /// Always print logs after the status table, not only on failure.
        #[arg(long)]
        logs_always: bool,

        /// Print debug logs instead of normal logs.
        #[arg(long)]
        debug_logs: bool,

        /// Save logs (normal and debug) to .logs/<run_id>/. Requires running
        /// inside a dbt project.
        #[arg(long)]
        save_files: bool,
    },

    /// Cancel a specific run by ID (running or queued)
    Cancel {
        /// ID of the run to cancel.
        run_id: String,

        /// Config scope: "local" (./.dbt-assist/) or "global". Omit to auto-detect.
        #[arg(long, value_enum)]
        scope: Option<ScopeArg>,

        /// Project the run belongs to (defaults to `name:` in dbt_project.yml;
        /// required when not run inside a dbt project).
        #[arg(long)]
        project_name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TemplatesSubcommands {
    /// List available templates. With no flag, shows predefined, user, and
    /// project templates; pass one or more flags to narrow the list.
    List {
        /// Show bundled, predefined templates.
        #[arg(long)]
        predefined: bool,

        /// Show user templates (global config directory).
        #[arg(long)]
        user: bool,

        /// Show project templates (./.templates/).
        #[arg(long)]
        project: bool,
    },

    /// Show a template's documentation and its output-path expression
    Docs {
        /// Name of the template.
        name: String,

        /// Source to read from: required when the name exists in more than one
        /// source.
        #[arg(long, value_enum)]
        source: Option<TemplateSourceArg>,
    },

    /// Render a template into a dbt model file
    ///
    /// Pass the template name followed by any number of `--key value` (or
    /// `--key=value`) variables. The reserved flags `--source <src>` and
    /// `--output <path>` may appear anywhere among the arguments.
    Build {
        /// Template name plus `--key value` variables and reserved flags.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 1..)]
        args: Vec<String>,
    },
}
