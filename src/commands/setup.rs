use crate::models::config::{
    AppConfig, ConfigScope, DbtApiConnection, ManifestStorage, config_dir, load_config, save_config,
};
use crate::vprintln;
use colored::Colorize;
use dialoguer::{Confirm, Input, Password, Select};
use std::env;
use std::io::Write;

pub fn setup(test_only: bool, scope: Option<ConfigScope>) {
    vprintln!("Verbose mode enabled");

    let (disc_path, disc_scope) = match config_dir(None) {
        Ok(resolved) => resolved,
        Err(e) => {
            eprintln!(
                "{} Could not resolve config directory: {e}",
                "error:".red().bold()
            );
            return;
        }
    };
    let disc_exists = disc_path.join("config.yaml").exists();
    vprintln!(
        "Discovered {disc_scope} config dir at {} (config.yaml exists: {disc_exists})",
        disc_path.display()
    );

    let target_scope = match (scope, disc_exists) {
        (Some(s), _) => s,
        (None, true) => disc_scope,
        (None, false) => ask_user_for_scope(),
    };

    let (target_path, _) = match config_dir(Some(target_scope)) {
        Ok(resolved) => resolved,
        Err(e) => {
            eprintln!(
                "{} Could not resolve {target_scope} config directory: {e}",
                "error:".red().bold()
            );
            return;
        }
    };
    let target_yaml = target_path.join("config.yaml");
    let target_exists = target_yaml.exists();

    if test_only {
        vprintln!("Test-only mode: validating existing config without modifying it");
        if !target_exists {
            eprintln!(
                "{} No config at {}. Run `dbt-assist setup` first.",
                "error:".red().bold(),
                target_yaml.display().to_string().cyan()
            );
            return;
        }
        println!(
            "Testing {} config at {}",
            target_scope.to_string().bold(),
            target_yaml.display().to_string().cyan()
        );
        match load_config(Some(target_scope)) {
            Ok((config, _)) => test_config(&config),
            Err(e) => eprintln!("{} Could not load config: {e}", "error:".red().bold()),
        }
        return;
    }

    let verb = if target_exists { "Using" } else { "Creating" };
    println!(
        "{verb} {} config at {}",
        target_scope.to_string().bold(),
        target_yaml.display().to_string().cyan()
    );

    if target_exists {
        let modify = Confirm::new()
            .with_prompt("Modify existing config?")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !modify {
            println!("{}", "Keeping existing config.".dimmed());
            match load_config(Some(target_scope)) {
                Ok((config, _)) => test_config(&config),
                Err(e) => eprintln!("{} Could not load config: {e}", "error:".red().bold()),
            }
            return;
        }
    }

    let service_account_path = setup_service_account();
    let dbt_api_connection = setup_dbt_api_connection();
    let manifest_storage = setup_manifest_storage();
    let project = setup_project();

    let config = AppConfig {
        dbt_api_connection,
        manifest_storage,
        service_account_path,
        project,
    };

    vprintln!("Saving {target_scope} config to {}", target_yaml.display());
    let saved_scope = match save_config(&config, Some(target_scope)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{} Could not save config: {e}", "error:".red().bold());
            return;
        }
    };
    println!(
        "{} {} config saved to {}",
        "✓".green().bold(),
        saved_scope.to_string().bold(),
        target_yaml.display().to_string().cyan()
    );

    test_config(&config);
}

fn ask_user_for_scope() -> ConfigScope {
    println!("\n{}", "› Config scope".bold().cyan());
    let options = ["Local (./.dbt-assist/)", "Global (user config dir)"];
    let choice = Select::new()
        .with_prompt("Which scope?")
        .items(options)
        .default(0)
        .interact()
        .unwrap();
    if choice == 0 {
        ConfigScope::Local
    } else {
        ConfigScope::Global
    }
}

fn setup_service_account() -> Option<String> {
    println!("\n{}", "› Service account".bold().cyan());
    if let Ok(env_path) = env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        let use_env = Confirm::new()
            .with_prompt(format!(
                "Use GOOGLE_APPLICATION_CREDENTIALS ({env_path}) as the service account?"
            ))
            .default(true)
            .interact()
            .unwrap_or(false);
        if use_env {
            return Some(env_path);
        }
    }

    let path: String = Input::new()
        .with_prompt("Path to service account JSON file")
        .interact_text()
        .unwrap();
    Some(path.trim().to_string())
}

fn setup_dbt_api_connection() -> DbtApiConnection {
    println!("\n{}", "› dbt API connection".bold().cyan());
    let options = ["Direct", "Normal proxy", "GCP Cloud Function proxy"];
    let choice = Select::new()
        .with_prompt("Connection type")
        .items(options)
        .default(0)
        .interact()
        .unwrap();

    match choice {
        0 => {
            let dbt_api_url: String = Input::new()
                .with_prompt("dbt API URL")
                .interact_text()
                .unwrap();
            let dbt_api_token: String = Input::new()
                .with_prompt("dbt API token")
                .interact_text()
                .unwrap();
            let account_id: i64 = Input::new()
                .with_prompt("dbt account ID")
                .interact_text()
                .unwrap();
            let dbt_assist_job_name: String = Input::new()
                .with_prompt("Name of the dbt-assist job")
                .interact_text()
                .unwrap();
            let dbt_target_name: String = Input::new()
                .with_prompt("dbt target name")
                .default("prod".to_string())
                .interact_text()
                .unwrap();
            let username: String = Input::new()
                .with_prompt("Username to link runs with (optional)")
                .allow_empty(true)
                .interact_text()
                .unwrap();
            let username = {
                let trimmed = username.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            };
            DbtApiConnection::Direct {
                dbt_api_url: dbt_api_url.trim().to_string(),
                dbt_api_token: dbt_api_token.trim().to_string(),
                account_id,
                dbt_assist_job_name: dbt_assist_job_name.trim().to_string(),
                dbt_target_name: dbt_target_name.trim().to_string(),
                username,
                default_threads_num: None,
                turbo_threads_num: None,
            }
        }
        1 => {
            let proxy_url: String = Input::new()
                .with_prompt("Proxy URL")
                .interact_text()
                .unwrap();
            let use_auth = Confirm::new()
                .with_prompt("Use auth for the proxy?")
                .default(false)
                .interact()
                .unwrap_or(false);
            let (proxy_username, proxy_password) = if use_auth {
                let username: String = Input::new()
                    .with_prompt("Proxy username")
                    .interact_text()
                    .unwrap();
                let password: String = Password::new()
                    .with_prompt("Proxy password")
                    .interact()
                    .unwrap();
                (
                    Some(username.trim().to_string()),
                    Some(password.trim().to_string()),
                )
            } else {
                (None, None)
            };
            DbtApiConnection::NormalProxy {
                proxy_url: proxy_url.trim().to_string(),
                proxy_username,
                proxy_password,
            }
        }
        2 => {
            let endpoint_url: String = Input::new()
                .with_prompt("Cloud Function endpoint URL")
                .interact_text()
                .unwrap();
            let auth_with_service_account = Confirm::new()
                .with_prompt("Authenticate via the service account configured above?")
                .default(true)
                .interact()
                .unwrap_or(false);
            DbtApiConnection::GcpFunctionProxy {
                endpoint_url: endpoint_url.trim().to_string(),
                auth_with_service_account,
            }
        }
        _ => unreachable!(),
    }
}

fn setup_manifest_storage() -> ManifestStorage {
    println!("\n{}", "› manifest.json storage".bold().cyan());
    let options = ["Local", "GCS"];
    let choice = Select::new()
        .with_prompt("Storage type")
        .items(options)
        .default(0)
        .interact()
        .unwrap();

    match choice {
        0 => {
            let path: String = Input::new()
                .with_prompt("Directory for manifest.json")
                .interact_text()
                .unwrap();
            ManifestStorage::Local {
                path: path.trim().to_string(),
            }
        }
        1 => {
            let bucket: String = Input::new()
                .with_prompt("GCS bucket name")
                .interact_text()
                .unwrap();
            let path: String = Input::new()
                .with_prompt("Path inside the bucket for manifest.json")
                .interact_text()
                .unwrap();
            let test_file: String = Input::new()
                .with_prompt("Path to a test file (used to verify bucket access)")
                .interact_text()
                .unwrap();
            ManifestStorage::GCS {
                bucket: bucket.trim().to_string(),
                path: path.trim().to_string(),
                test_file: test_file.trim().to_string(),
            }
        }
        _ => unreachable!(),
    }
}

fn setup_project() -> Option<String> {
    println!("\n{}", "› GCP project".bold().cyan());
    let project: String = Input::new()
        .with_prompt("GCP project ID (optional — defaults to the service account's project)")
        .allow_empty(true)
        .interact_text()
        .unwrap();
    let trimmed = project.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn test_config(config: &AppConfig) {
    println!("\n{}", "› Config validation".bold().cyan());
    vprintln!("Starting config validation tests");
    describe_config(config);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");
    rt.block_on(async {
        // The service account is needed by GCS manifest storage and by a GCP
        // Cloud Function proxy that authenticates via the service account.
        // Validate it once up front and share the result with both checks.
        let needs_sa = matches!(config.manifest_storage, ManifestStorage::GCS { .. })
            || matches!(
                config.dbt_api_connection,
                DbtApiConnection::GcpFunctionProxy {
                    auth_with_service_account: true,
                    ..
                }
            );
        let sa_ok = if needs_sa {
            check_service_account(config).await
        } else {
            true
        };

        check_dbt_connection(config, sa_ok).await;

        match &config.manifest_storage {
            ManifestStorage::Local { .. } => check_local_access(config),
            ManifestStorage::GCS { .. } => {
                if sa_ok {
                    check_gcs_access(config).await;
                }
            }
        }
    });
}

/// Logs (in verbose mode) the exact config being exercised by the validation
/// tests: which service account, which kind of manifest storage, and which kind
/// of dbt API connection.
fn describe_config(config: &AppConfig) {
    match &config.service_account_path {
        Some(path) => vprintln!("  service account: {path}"),
        None => vprintln!("  service account: <none>"),
    }

    match &config.manifest_storage {
        ManifestStorage::Local { path } => {
            vprintln!("  manifest storage: local ({path})");
        }
        ManifestStorage::GCS {
            bucket,
            path,
            test_file,
        } => {
            vprintln!(
                "  manifest storage: gcs (bucket: {bucket}, path: {path}, test file: {test_file})"
            );
        }
    }

    match &config.dbt_api_connection {
        DbtApiConnection::Direct { dbt_api_url, .. } => {
            vprintln!("  dbt API connection: direct ({dbt_api_url})");
        }
        DbtApiConnection::NormalProxy {
            proxy_url,
            proxy_username,
            proxy_password,
        } => {
            vprintln!(
                "  dbt API connection: normal proxy ({proxy_url}, auth: {})",
                if proxy_username.is_some() && proxy_password.is_some() {
                    "yes"
                } else {
                    "no"
                }
            );
        }
        DbtApiConnection::GcpFunctionProxy {
            endpoint_url,
            auth_with_service_account,
        } => {
            vprintln!(
                "  dbt API connection: gcp cloud function proxy ({endpoint_url}, service account auth: {auth_with_service_account})"
            );
        }
    }
}

/// Validates the configured dbt API connection with a ping. When the connection
/// authenticates via the service account, the ping can only work if the service
/// account itself loaded, so we gate on `sa_ok`.
async fn check_dbt_connection(config: &AppConfig, sa_ok: bool) {
    print!("  dbt API ... ");
    std::io::stdout().flush().ok();

    let needs_sa = matches!(
        config.dbt_api_connection,
        DbtApiConnection::GcpFunctionProxy {
            auth_with_service_account: true,
            ..
        }
    );
    if needs_sa && !sa_ok {
        println!(
            "{}\n    {}",
            "skipped".yellow(),
            "service account check failed".dimmed()
        );
        return;
    }

    match ping_connection(config).await {
        Ok(()) => println!("{}", "✓".green().bold()),
        Err(e) => println!("{}\n    {}", "✗".red().bold(), e.to_string().red()),
    }
}

async fn ping_connection(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    use crate::api::client::{DbtApi, DbtApiClient};
    let api = DbtApi::from_config(config)?;
    api.ping().await
}

/// Prints the service account validation result and returns whether it passed.
async fn check_service_account(config: &AppConfig) -> bool {
    print!("  Service account ... ");
    std::io::stdout().flush().ok();
    match try_load_service_account(config).await {
        Ok(()) => {
            println!("{}", "✓".green().bold());
            true
        }
        Err(e) => {
            println!("{}\n    {}", "✗".red().bold(), e.to_string().red());
            false
        }
    }
}

async fn try_load_service_account(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let path = crate::gcp::client::get_service_account_path(config)?;
    crate::gcp::client::load_service_account(&path).await?;
    Ok(())
}

/// Verifies GCS bucket access by downloading the configured test file. No-op
/// for local manifest storage, which has nothing to validate against GCS.
async fn check_gcs_access(config: &AppConfig) {
    let ManifestStorage::GCS {
        bucket,
        path,
        test_file,
    } = &config.manifest_storage
    else {
        return;
    };
    print!("  GCS bucket ... ");
    std::io::stdout().flush().ok();
    let object = join_object_key(path, test_file);
    match crate::gcp::client::download_object(config, bucket, &object).await {
        Ok(_) => println!("{}", "✓".green().bold()),
        Err(e) => println!("{}\n    {}", "✗".red().bold(), e.to_string().red()),
    }
}

/// Verifies the local manifest directory exists and is readable. No-op for GCS
/// manifest storage. Does not require the service account.
fn check_local_access(config: &AppConfig) {
    let ManifestStorage::Local { path } = &config.manifest_storage else {
        return;
    };
    print!("  Local manifest dir ... ");
    std::io::stdout().flush().ok();
    match verify_local_dir(path) {
        Ok(()) => println!("{}", "✓".green().bold()),
        Err(e) => println!("{}\n    {}", "✗".red().bold(), e.to_string().red()),
    }
}

/// Confirms `path` exists, is a directory, and can be read. Reading the
/// directory entries is what actually exercises read permission.
fn verify_local_dir(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dir = crate::utils::expand_tilde(path);
    if !dir.exists() {
        return Err(format!("Directory not found: {path}").into());
    }
    if !dir.is_dir() {
        return Err(format!("Not a directory: {path}").into());
    }
    std::fs::read_dir(&dir)?; // surfaces permission errors as the `?` error
    Ok(())
}

/// Joins the manifest `path` and `test_file` into a single GCS object key,
/// collapsing redundant slashes and handling an empty path.
fn join_object_key(path: &str, test_file: &str) -> String {
    let path = path.trim_matches('/');
    let file = test_file.trim_start_matches('/');
    if path.is_empty() {
        file.to_string()
    } else {
        format!("{path}/{file}")
    }
}

#[cfg(test)]
mod tests {
    use super::{join_object_key, verify_local_dir};

    #[test]
    fn verify_local_dir_accepts_existing_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(verify_local_dir(dir.path().to_str().unwrap()).is_ok());
    }

    #[test]
    fn verify_local_dir_rejects_missing_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(verify_local_dir(missing.to_str().unwrap()).is_err());
    }

    #[test]
    fn verify_local_dir_rejects_a_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(verify_local_dir(file.path().to_str().unwrap()).is_err());
    }

    #[test]
    fn join_object_key_combines_path_and_file() {
        assert_eq!(
            join_object_key("prefix/manifest", "test.json"),
            "prefix/manifest/test.json"
        );
    }

    #[test]
    fn join_object_key_trims_trailing_slash_on_path() {
        assert_eq!(
            join_object_key("prefix/manifest/", "test.json"),
            "prefix/manifest/test.json"
        );
    }

    #[test]
    fn join_object_key_trims_leading_slash_on_file() {
        assert_eq!(
            join_object_key("prefix/manifest", "/test.json"),
            "prefix/manifest/test.json"
        );
    }

    #[test]
    fn join_object_key_handles_empty_path() {
        assert_eq!(join_object_key("", "test.json"), "test.json");
    }
}
