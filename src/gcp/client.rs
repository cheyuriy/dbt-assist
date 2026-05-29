use crate::models::config::AppConfig;
use google_cloud_storage::client::{
    Client, ClientConfig, google_cloud_auth::credentials::CredentialsFile,
};
use google_cloud_storage::http::objects::{download::Range, get::GetObjectRequest};
use std::path::{Path, PathBuf};
use std::{env, ops::Deref};

pub async fn get_client(
    config: &AppConfig,
) -> Result<(Client, String), Box<dyn std::error::Error>> {
    let service_account_path = get_service_account_path(config)?;

    let client_config = load_service_account(&service_account_path).await?;

    let project_id = if config.project.is_some() {
        config.project.as_ref().unwrap().deref().to_string()
    } else if client_config.project_id.is_some() {
        client_config
            .project_id
            .as_ref()
            .unwrap()
            .deref()
            .to_string()
    } else {
        return Err("Project ID not found in configuration or service account credentials".into());
    };

    let gcs_client = Client::new(client_config);

    Ok((gcs_client, project_id))
}

pub fn get_service_account_path(config: &AppConfig) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(service_account_path) = &config.service_account_path {
        let res = PathBuf::from(service_account_path);
        if res.exists() {
            Ok(res)
        } else {
            Err("Service account file not found".into())
        }
    } else if let Ok(env_path) = env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        let res = PathBuf::from(env_path);
        if res.exists() {
            Ok(res)
        } else {
            Err("Service account file not found".into())
        }
    } else {
        Err("Service account file not found".into())
    }
}

pub async fn load_service_account(
    credentials_path: &Path,
) -> Result<ClientConfig, Box<dyn std::error::Error>> {
    let credentials_file = match CredentialsFile::new_from_file(
        credentials_path.to_str().unwrap().to_string(),
    )
    .await
    {
        Ok(cred) => cred,
        Err(_) => return Err("Service account file not found".into()),
    };
    let config = ClientConfig::default()
        .with_credentials(credentials_file)
        .await
        .unwrap();
    Ok(config)
}

/// Downloads the bytes of `object` from `bucket` using the credentials in
/// `config`. Used to verify GCS bucket access during setup validation.
pub async fn download_object(
    config: &AppConfig,
    bucket: &str,
    object: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (client, _project_id) = get_client(config).await?;
    let data = client
        .download_object(
            &GetObjectRequest {
                bucket: bucket.to_string(),
                object: object.to_string(),
                ..Default::default()
            },
            &Range::default(),
        )
        .await?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::config::{AppConfig, DbtApiConnection, ManifestStorage};
    use serial_test::serial;

    /// Loads `.env.test` from the crate root (best-effort) and returns the
    /// path stored under `TEST_SERVICE_ACCOUNT_PATH`. Tests that need a real
    /// service account call this; tests that only exercise path/error logic
    /// do not.
    fn test_service_account_path() -> String {
        let _ = dotenvy::from_filename(".env.test");
        env::var("TEST_SERVICE_ACCOUNT_PATH").unwrap_or_else(|_| {
            panic!(
                "{} must be set (define it in .env.test or in the environment)",
                "TEST_SERVICE_ACCOUNT_PATH"
            )
        })
    }

    /// Returns the GCS bucket name for real-download tests, sourced from
    /// `TEST_GCS_BUCKET` (define it in `.env.test` or the environment).
    fn test_gcs_bucket() -> String {
        let _ = dotenvy::from_filename(".env.test");
        env::var("TEST_GCS_BUCKET").unwrap_or_else(|_| panic!("{} must be set", "TEST_GCS_BUCKET"))
    }

    /// Returns the object key (path inside the bucket) for real-download tests,
    /// sourced from `TEST_GCS_OBJECT`.
    fn test_gcs_object() -> String {
        let _ = dotenvy::from_filename(".env.test");
        env::var("TEST_GCS_OBJECT").unwrap_or_else(|_| panic!("{} must be set", "TEST_GCS_OBJECT"))
    }

    fn make_config(service_account_path: Option<String>, project: Option<String>) -> AppConfig {
        AppConfig {
            dbt_api_connection: DbtApiConnection::Direct {
                dbt_api_url: "https://api.example.com".to_string(),
                dbt_api_token: "tok".to_string(),
            },
            manifest_storage: ManifestStorage::Local {
                path: "/var/manifest".to_string(),
            },
            service_account_path,
            project,
        }
    }

    /// RAII guard to override `GOOGLE_APPLICATION_CREDENTIALS` for the
    /// duration of a test and restore the previous value on drop. Pair with
    /// `#[serial]` — env vars are process-global and `set_var` is `unsafe` in
    /// Rust 2024 for that reason.
    struct GoogleApplicationCredentialsEnv {
        prev: Option<String>,
    }

    impl GoogleApplicationCredentialsEnv {
        fn set(value: &str) -> Self {
            let prev = env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
            // SAFETY: serialized across tests via `#[serial]`.
            unsafe { env::set_var("GOOGLE_APPLICATION_CREDENTIALS", value) };
            Self { prev }
        }

        fn unset() -> Self {
            let prev = env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
            // SAFETY: serialized across tests via `#[serial]`.
            unsafe { env::remove_var("GOOGLE_APPLICATION_CREDENTIALS") };
            Self { prev }
        }
    }

    impl Drop for GoogleApplicationCredentialsEnv {
        fn drop(&mut self) {
            // SAFETY: serialized across tests via `#[serial]`.
            unsafe {
                match &self.prev {
                    Some(v) => env::set_var("GOOGLE_APPLICATION_CREDENTIALS", v),
                    None => env::remove_var("GOOGLE_APPLICATION_CREDENTIALS"),
                }
            }
        }
    }

    #[test]
    #[serial]
    fn service_account_path_from_config() {
        let _g = GoogleApplicationCredentialsEnv::unset();
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        let path_str = tmp.path().to_str().unwrap().to_string();
        let config = make_config(Some(path_str.clone()), None);

        let resolved = get_service_account_path(&config).expect("get path");
        assert_eq!(resolved, PathBuf::from(&path_str));
    }

    #[test]
    #[serial]
    fn service_account_path_errors_when_config_path_missing() {
        let _g = GoogleApplicationCredentialsEnv::unset();
        let config = make_config(Some("/does/not/exist.json".to_string()), None);
        assert!(get_service_account_path(&config).is_err());
    }

    #[test]
    #[serial]
    fn service_account_path_does_not_fall_back_when_config_path_invalid() {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        let _g = GoogleApplicationCredentialsEnv::set(tmp.path().to_str().unwrap());
        let config = make_config(Some("/does/not/exist.json".to_string()), None);
        assert!(get_service_account_path(&config).is_err());
    }

    #[test]
    #[serial]
    fn service_account_path_falls_back_to_env() {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        let path_str = tmp.path().to_str().unwrap().to_string();
        let _g = GoogleApplicationCredentialsEnv::set(&path_str);
        let config = make_config(None, None);

        let resolved = get_service_account_path(&config).expect("get path");
        assert_eq!(resolved, PathBuf::from(&path_str));
    }

    #[test]
    #[serial]
    fn service_account_path_errors_when_env_path_missing() {
        let _g = GoogleApplicationCredentialsEnv::set("/does/not/exist.json");
        let config = make_config(None, None);
        assert!(get_service_account_path(&config).is_err());
    }

    #[test]
    #[serial]
    fn service_account_path_errors_when_no_config_and_no_env() {
        let _g = GoogleApplicationCredentialsEnv::unset();
        let config = make_config(None, None);
        assert!(get_service_account_path(&config).is_err());
    }

    #[tokio::test]
    #[serial]
    async fn load_service_account_succeeds_with_valid_path() {
        let path = test_service_account_path();
        load_service_account(Path::new(&path))
            .await
            .expect("load service account");
    }

    #[tokio::test]
    #[serial]
    async fn load_service_account_errors_with_invalid_path() {
        let result = load_service_account(Path::new("/does/not/exist.json")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[serial]
    async fn get_client_uses_project_from_config() {
        let _g = GoogleApplicationCredentialsEnv::unset();
        let sa_path = test_service_account_path();
        let config = make_config(Some(sa_path), Some("config-project".to_string()));

        let (_client, project_id) = get_client(&config).await.expect("get client");
        assert_eq!(project_id, "config-project");
    }

    #[tokio::test]
    #[serial]
    async fn get_client_falls_back_to_service_account_project() {
        let _g = GoogleApplicationCredentialsEnv::unset();
        let sa_path = test_service_account_path();
        let config = make_config(Some(sa_path), None);

        let (_client, project_id) = get_client(&config).await.expect("get client");
        assert!(
            !project_id.is_empty(),
            "project id should be sourced from the service account credentials"
        );
    }

    #[tokio::test]
    #[serial]
    async fn get_client_resolves_service_account_from_env() {
        let sa_path = test_service_account_path();
        let _g = GoogleApplicationCredentialsEnv::set(&sa_path);
        let config = make_config(None, Some("config-project".to_string()));

        let (_client, project_id) = get_client(&config).await.expect("get client");
        assert_eq!(project_id, "config-project");
    }

    #[tokio::test]
    #[serial]
    async fn get_client_errors_without_service_account() {
        let _g = GoogleApplicationCredentialsEnv::unset();
        let config = make_config(None, None);
        assert!(get_client(&config).await.is_err());
    }

    #[tokio::test]
    #[serial]
    async fn download_object_succeeds_for_existing_object() {
        let _g = GoogleApplicationCredentialsEnv::unset();
        let config = make_config(Some(test_service_account_path()), None);
        let bucket = test_gcs_bucket();
        let object = test_gcs_object();

        download_object(&config, &bucket, &object)
            .await
            .expect("download existing object");
    }

    #[tokio::test]
    #[serial]
    async fn download_object_errors_for_missing_object() {
        let _g = GoogleApplicationCredentialsEnv::unset();
        let config = make_config(Some(test_service_account_path()), None);
        let bucket = test_gcs_bucket();

        let result =
            download_object(&config, &bucket, "does/not/exist/dbt-assist-missing.json").await;
        assert!(result.is_err());
    }
}
