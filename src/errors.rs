//! Concrete error types for dbt-assist, grouped by where the error happens.
//!
//! Each layer has its own error enum so callers can tell *what* failed, not just
//! read a string. The `DbtApiClient` trait still hands back
//! `Box<dyn std::error::Error>` — the connectors produce these concrete enums and
//! box them at the trait boundary. Command entry points and the few helpers that
//! genuinely compose errors across layers also keep `Box<dyn std::error::Error>`.

use thiserror::Error;

/// Google's auth-crate error (credentials loading, ID-token minting). Re-exported
/// through `google-cloud-storage`, which is the dependency we pull it from.
type GoogleAuthError = google_cloud_storage::client::google_cloud_auth::error::Error;

// ---------------------------------------------------------------------------
// Models layer: two shared error kinds reused across the codebase.
// ---------------------------------------------------------------------------

/// User-facing validation of names and template content: alias/template name
/// rules, template custom-tag parsing, and template rendering. None of these
/// touch the filesystem — they are "the input is wrong" errors.
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("{kind} name must not be empty")]
    EmptyName { kind: &'static str },
    #[error("{kind} name must not contain '/', '\\', or '.'")]
    IllegalChars { kind: &'static str },
    #[error("template defines more than one {{% output %}} tag")]
    MultipleOutput,
    #[error("template defines more than one {{% docs %}} block")]
    MultipleDocs,
    #[error("{{% docs %}} block is not closed with {{% enddocs %}}")]
    UnclosedDocs,
    #[error("template render failed: {0}")]
    Render(#[from] minijinja::Error),
    /// Bad arguments to `templates build`; carries a pre-formatted message.
    #[error("{0}")]
    BuildArgs(String),
}

/// Working with directories and files: filesystem I/O, missing config/manifest,
/// config-directory resolution, and config (de)serialization.
#[derive(Error, Debug)]
pub enum EnvironmentError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("config file not found at {0}")]
    ConfigNotFound(String),
    #[error("manifest not found at {0}")]
    ManifestNotFound(String),
    #[error("could not determine default config directory")]
    NoDefaultConfigDir,
    #[error("config directory specified in environment variable does not exist")]
    EnvConfigDirMissing,
    #[error("failed to read config: {0}")]
    ConfigParse(#[from] config::ConfigError),
    #[error("failed to serialize config: {0}")]
    Yaml(#[from] serde_yml::Error),
}

// ---------------------------------------------------------------------------
// GCP layer.
// ---------------------------------------------------------------------------

/// Service-account loading and GCS access (`src/gcp/client.rs`).
#[derive(Error, Debug)]
pub enum GcpError {
    #[error("service account file not found")]
    ServiceAccountNotFound,
    #[error("project ID not found in configuration or service account credentials")]
    ProjectIdNotFound,
    #[error("failed to load service account credentials: {0}")]
    Credentials(#[from] GoogleAuthError),
    #[error("GCS request failed: {0}")]
    Storage(#[from] google_cloud_storage::http::Error),
}

// ---------------------------------------------------------------------------
// dbt API layer.
// ---------------------------------------------------------------------------

/// The small custom proxy API spoken by both proxy connectors
/// (`src/api/proxy.rs`). JSON-decode failures from `reqwest` arrive as
/// `reqwest::Error`, so they fold into `Http`.
#[derive(Error, Debug)]
pub enum ProxyError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// A non-success HTTP status, pre-formatted with context and the proxy's
    /// `{"message": ...}` body when present.
    #[error("{0}")]
    BadStatus(String),
}

/// Plain proxy connector (`src/api/normal_proxy.rs`). Basic auth is infallible,
/// so this only wraps [`ProxyError`] plus its own ping status.
#[derive(Error, Debug)]
pub enum NormalProxyError {
    #[error(transparent)]
    Proxy(#[from] ProxyError),
    #[error("{0}")]
    BadStatus(String),
}

/// GCP Cloud Function proxy connector (`src/api/gcp_function_proxy.rs`). Wraps
/// [`ProxyError`] and adds the auth failures specific to minting a GCP ID token.
#[derive(Error, Debug)]
pub enum GcpFunctionProxyError {
    #[error(transparent)]
    Proxy(#[from] ProxyError),
    #[error(transparent)]
    ServiceAccount(#[from] GcpError),
    #[error("failed to mint GCP ID token: {0}")]
    IdToken(#[from] GoogleAuthError),
    #[error("{0}")]
    BadStatus(String),
}

/// Direct dbt Cloud Admin API v2 connector (`src/api/direct.rs`). JSON-decode
/// failures arrive as `reqwest::Error` and fold into `Http`.
#[derive(Error, Debug)]
pub enum DirectError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// A non-success HTTP status, pre-formatted with context and response body.
    #[error("{0}")]
    BadStatus(String),
    #[error("project '{0}' not found in dbt account")]
    ProjectNotFound(String),
    #[error("job '{job}' not found in project {project_id}")]
    JobNotFound { job: String, project_id: i64 },
    #[error("cancel run did not take effect; run status is {0}")]
    CancelNotEffective(i64),
}
