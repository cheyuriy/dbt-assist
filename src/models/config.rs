use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub struct AppConfig {
    pub dbt_api_connection: DbtApiConnection,
    pub manifest_storage: ManifestStorage,
    pub service_account_path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DbtApiConnection {
    Direct {
        dbt_api_url: String,
        dbt_api_token: String,
    },

    GcpFunctionProxy {
        endpoint_url: String,
        auth_with_service_account: bool,
    },

    NormalProxy {
        proxy_url: String,
        proxy_key: Option<String>,
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ManifestStorage {
    Local {
        path: String,
    },

    #[allow(clippy::upper_case_acronyms)]
    #[serde(rename = "gcs")]
    GCS {
        bucket: String,
        path: String,
        test_file: String,
    },
}

impl AppConfig {
    pub fn new(dbt_api_connection: DbtApiConnection, manifest_storage: ManifestStorage, service_account_path: Option<String>) -> Self {
        AppConfig {
            dbt_api_connection,
            manifest_storage,
            service_account_path,
        }
    }

    pub fn load_from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let config_content = std::fs::read_to_string(path)?;
        let config: AppConfig = serde_yml::from_str::<AppConfig>(&config_content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_config(yaml: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).expect("write config file");
        (dir, path)
    }

    #[test]
    fn new_sets_all_fields() {
        let config = AppConfig::new(
            DbtApiConnection::Direct {
                dbt_api_url: "https://example.com".to_string(),
                dbt_api_token: "tok".to_string(),
            },
            ManifestStorage::Local {
                path: "/tmp/manifest".to_string(),
            },
            Some("/path/to/sa.json".to_string()),
        );

        match config.dbt_api_connection {
            DbtApiConnection::Direct { dbt_api_url, dbt_api_token } => {
                assert_eq!(dbt_api_url, "https://example.com");
                assert_eq!(dbt_api_token, "tok");
            }
            _ => panic!("expected Direct variant"),
        }
        match config.manifest_storage {
            ManifestStorage::Local { path } => assert_eq!(path, "/tmp/manifest"),
            _ => panic!("expected Local variant"),
        }
        assert_eq!(config.service_account_path.as_deref(), Some("/path/to/sa.json"));
    }

    #[test]
    fn load_direct_connection() {
        let yaml = r#"
dbt_api_connection:
  type: direct
  dbt_api_url: https://api.example.com
  dbt_api_token: secret-token
manifest_storage:
  type: local
  path: /var/manifest
service_account_path: /sa.json
"#;
        let (_dir, path) = write_config(yaml);
        let config = AppConfig::load_from_file(&path).expect("load config");

        match config.dbt_api_connection {
            DbtApiConnection::Direct { dbt_api_url, dbt_api_token } => {
                assert_eq!(dbt_api_url, "https://api.example.com");
                assert_eq!(dbt_api_token, "secret-token");
            }
            _ => panic!("expected Direct variant"),
        }
        assert_eq!(config.service_account_path.as_deref(), Some("/sa.json"));
    }

    #[test]
    fn load_gcp_function_proxy_connection() {
        let yaml = r#"
dbt_api_connection:
  type: gcp_function_proxy
  endpoint_url: https://gcp.example.com/fn
  auth_with_service_account: true
manifest_storage:
  type: local
  path: /var/manifest
"#;
        let (_dir, path) = write_config(yaml);
        let config = AppConfig::load_from_file(&path).expect("load config");

        match config.dbt_api_connection {
            DbtApiConnection::GcpFunctionProxy { endpoint_url, auth_with_service_account } => {
                assert_eq!(endpoint_url, "https://gcp.example.com/fn");
                assert!(auth_with_service_account);
            }
            _ => panic!("expected GcpFunctionProxy variant"),
        }
    }

    #[test]
    fn load_normal_proxy_connection() {
        let yaml = r#"
dbt_api_connection:
  type: normal_proxy
  proxy_url: https://proxy.example.com
manifest_storage:
  type: local
  path: /var/manifest
"#;
        let (_dir, path) = write_config(yaml);
        let config = AppConfig::load_from_file(&path).expect("load config");

        match config.dbt_api_connection {
            DbtApiConnection::NormalProxy { proxy_url, proxy_key } => {
                assert_eq!(proxy_url, "https://proxy.example.com");
                assert!(proxy_key.is_none(), "proxy_key should default to None when omitted");
            }
            _ => panic!("expected NormalProxy variant"),
        }
    }

    #[test]
    fn load_local_manifest_storage() {
        let yaml = r#"
dbt_api_connection:
  type: direct
  dbt_api_url: https://api.example.com
  dbt_api_token: tok
manifest_storage:
  type: local
  path: /var/manifest
"#;
        let (_dir, path) = write_config(yaml);
        let config = AppConfig::load_from_file(&path).expect("load config");

        match config.manifest_storage {
            ManifestStorage::Local { path } => assert_eq!(path, "/var/manifest"),
            _ => panic!("expected Local variant"),
        }
    }

    #[test]
    fn load_gcs_manifest_storage() {
        let yaml = r#"
dbt_api_connection:
  type: direct
  dbt_api_url: https://api.example.com
  dbt_api_token: tok
manifest_storage:
  type: gcs
  bucket: my-bucket
  path: prefix/manifest
  test_file: prefix/.healthcheck
"#;
        let (_dir, path) = write_config(yaml);
        let config = AppConfig::load_from_file(&path).expect("load config");

        match config.manifest_storage {
            ManifestStorage::GCS { bucket, path, test_file } => {
                assert_eq!(bucket, "my-bucket");
                assert_eq!(path, "prefix/manifest");
                assert_eq!(test_file, "prefix/.healthcheck");
            }
            _ => panic!("expected GCS variant"),
        }
    }

    #[test]
    fn load_without_service_account_path() {
        let yaml = r#"
dbt_api_connection:
  type: direct
  dbt_api_url: https://api.example.com
  dbt_api_token: tok
manifest_storage:
  type: local
  path: /var/manifest
"#;
        let (_dir, path) = write_config(yaml);
        let config = AppConfig::load_from_file(&path).expect("load config");

        assert!(config.service_account_path.is_none());
    }

    #[test]
    fn load_from_nonexistent_file_errors() {
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join("does-not-exist.yaml");
        assert!(AppConfig::load_from_file(&path).is_err());
    }

    #[test]
    fn load_from_malformed_yaml_errors() {
        let (_dir, path) = write_config("::: not yaml :::");
        assert!(AppConfig::load_from_file(&path).is_err());
    }

    #[test]
    fn load_with_unknown_connection_type_errors() {
        let yaml = r#"
dbt_api_connection:
  type: bogus
  some_field: value
manifest_storage:
  type: local
  path: /var/manifest
"#;
        let (_dir, path) = write_config(yaml);
        assert!(AppConfig::load_from_file(&path).is_err());
    }
}

