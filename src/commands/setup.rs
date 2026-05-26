use crate::models::config::{
    AppConfig, DbtApiConnection, ManifestStorage, config_dir, load_config, save_config,
};
use crate::vprintln;
use dialoguer::{Confirm, Input, Select};
use std::env;

pub fn setup(test_only: bool) {
    vprintln!("Verbose mode enabled");

    let dir = match config_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to resolve config directory: {e}");
            return;
        }
    };
    let path = dir.join("config.yaml");
    vprintln!("Config path: {}", path.display());
    let exists = path.exists();

    if test_only {
        if !exists {
            eprintln!(
                "No config found at {}. Run `dbt-assist setup` first.",
                path.display()
            );
            return;
        }
        match load_config() {
            Ok(config) => test_config(&config),
            Err(e) => eprintln!("Failed to load config: {e}"),
        }
        return;
    }

    if exists {
        println!("Config already exists at {}.", path.display());
        let overwrite = Confirm::new()
            .with_prompt("Overwrite existing config?")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !overwrite {
            println!("Keeping existing config. Exiting.");
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

    if let Err(e) = save_config(&config) {
        eprintln!("Failed to save config: {e}");
        return;
    }
    println!("Setup complete. Config saved to {}.", path.display());

    test_config(&config);
}

fn setup_service_account() -> Option<String> {
    println!("\n== Service account ==");
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
    println!("\n== dbt API connection ==");
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
            DbtApiConnection::Direct {
                dbt_api_url: dbt_api_url.trim().to_string(),
                dbt_api_token: dbt_api_token.trim().to_string(),
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
            let proxy_token = if use_auth {
                let token: String = Input::new()
                    .with_prompt("Proxy auth token")
                    .interact_text()
                    .unwrap();
                Some(token.trim().to_string())
            } else {
                None
            };
            DbtApiConnection::NormalProxy {
                proxy_url: proxy_url.trim().to_string(),
                proxy_token,
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
    println!("\n== manifest.json storage ==");
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
    println!("\n== GCP project ==");
    let project: String = Input::new()
        .with_prompt("GCP project id (optional, leave blank to use the one from the service account)")
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

fn test_config(_config: &AppConfig) {
    println!("\n== Config validation ==");
    println!("Config validation is not implemented yet.");
}
