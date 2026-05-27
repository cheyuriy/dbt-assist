use serde::{Serialize, Deserialize};
use config::{Config, Environment, File};
use std::path::PathBuf;
use std::env;

#[derive(Serialize, Deserialize, Debug)]
pub struct AppConfig {
    pub dbt_api_connection: DbtApiConnection,
    pub manifest_storage: ManifestStorage,
    pub service_account_path: Option<String>,
    pub project: Option<String>,
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
        proxy_token: Option<String>,
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

fn project_config_dir() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    let dir = cwd.join(format!(".{}", env!("CARGO_PKG_NAME")));
    if dir.is_dir() { Some(dir) } else { None }
}

fn global_config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(proj_dirs) =
        directories::ProjectDirs::from("com", "cheyuriydev", env!("CARGO_PKG_NAME"))
    {
        Ok(proj_dirs.config_dir().to_path_buf())
    } else {
        Err("Could not determine default config directory".into())
    }
}

pub fn config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(dir) = env::var(format!("{}_CONFIG_DIR", env!("CARGO_PKG_NAME").replace("-", "_").to_uppercase())) {
        let dir = PathBuf::from(dir);
        if dir.exists() {
            Ok(dir)
        } else {
            Err("Config directory specified in environment variable does not exist".into())
        }
    } else if let Some(dir) = project_config_dir() {
        Ok(dir)
    } else {
        global_config_dir()
    }
}

pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let dir = config_dir()?;
    let path = dir.join("config.yaml");
    let pkg_name = env!("CARGO_PKG_NAME").replace("-", "_").to_uppercase();

    let builder = Config::builder()
        .add_source(File::from(path.clone()).required(false))
        .add_source(Environment::with_prefix(pkg_name.as_str()).separator("__"));

    let config = builder.build()?;

    let config: AppConfig = config.try_deserialize()?;

    Ok(config)
}

pub fn save_config(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.yaml");
    let yaml = serde_yml::to_string(config)?;
    std::fs::write(path, yaml)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn env_var_name() -> String {
        format!(
            "{}_CONFIG_DIR",
            env!("CARGO_PKG_NAME").replace("-", "_").to_uppercase()
        )
    }

    /// Creates a tempdir, points `<PKG>_CONFIG_DIR` at it for the duration of the
    /// test, and restores the previous value on drop. Pair every test that uses
    /// it with `#[serial]` — env vars are process-global and `set_var` is
    /// `unsafe` in Rust 2024 for that reason.
    struct ConfigDirEnv {
        dir: TempDir,
        var: String,
        prev: Option<String>,
    }

    impl ConfigDirEnv {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("create tempdir");
            let var = env_var_name();
            let prev = env::var(&var).ok();
            // SAFETY: serialized across tests via `#[serial]`.
            unsafe { env::set_var(&var, dir.path()) };
            Self { dir, var, prev }
        }

        fn path(&self) -> &std::path::Path {
            self.dir.path()
        }

        fn write_config(&self, yaml: &str) {
            std::fs::write(self.dir.path().join("config.yaml"), yaml)
                .expect("write config file");
        }
    }

    impl Drop for ConfigDirEnv {
        fn drop(&mut self) {
            // SAFETY: serialized across tests via `#[serial]`.
            unsafe {
                match &self.prev {
                    Some(v) => env::set_var(&self.var, v),
                    None => env::remove_var(&self.var),
                }
            }
        }
    }

    /// Switches the process cwd to a tempdir for the duration of the test and
    /// restores the previous cwd on drop. Pair with `#[serial]` — cwd is
    /// process-global.
    struct CwdGuard {
        dir: TempDir,
        prev: PathBuf,
    }

    impl CwdGuard {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("create tempdir");
            let prev = env::current_dir().expect("read cwd");
            env::set_current_dir(dir.path()).expect("set cwd");
            Self { dir, prev }
        }

        fn path(&self) -> &std::path::Path {
            self.dir.path()
        }

        fn make_project_config_dir(&self) -> PathBuf {
            let project = self.dir.path().join(format!(".{}", env!("CARGO_PKG_NAME")));
            std::fs::create_dir_all(&project).expect("create project config dir");
            project
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.prev);
        }
    }

    fn clear_config_env() -> Option<String> {
        let var = env_var_name();
        let prev = env::var(&var).ok();
        // SAFETY: serialized across tests via `#[serial]`.
        unsafe { env::remove_var(&var) };
        prev
    }

    fn restore_config_env(prev: Option<String>) {
        let var = env_var_name();
        // SAFETY: serialized across tests via `#[serial]`.
        unsafe {
            match prev {
                Some(v) => env::set_var(&var, v),
                None => env::remove_var(&var),
            }
        }
    }

    #[test]
    #[serial]
    fn config_dir_uses_env_var_when_set() {
        let env = ConfigDirEnv::new();
        let resolved = config_dir().expect("config_dir");
        assert_eq!(resolved, env.path());
    }

    #[test]
    #[serial]
    fn config_dir_uses_project_dir_when_present() {
        let prev = clear_config_env();
        let cwd = CwdGuard::new();
        let project = cwd.make_project_config_dir();

        let resolved = config_dir().expect("config_dir");
        // Canonicalize because macOS resolves /tmp -> /private/tmp.
        assert_eq!(
            resolved.canonicalize().unwrap(),
            project.canonicalize().unwrap()
        );

        restore_config_env(prev);
    }

    #[test]
    #[serial]
    fn env_var_takes_priority_over_project_dir() {
        let env = ConfigDirEnv::new();
        let cwd = CwdGuard::new();
        let _project = cwd.make_project_config_dir();

        let resolved = config_dir().expect("config_dir");
        assert_eq!(resolved, env.path());
    }

    #[test]
    #[serial]
    fn config_dir_falls_back_to_global_without_project_dir() {
        let prev = clear_config_env();
        let cwd = CwdGuard::new(); // cwd has no `.dbt-assist`

        let resolved = config_dir().expect("config_dir");
        let expected = global_config_dir().expect("global_config_dir");
        assert_eq!(resolved, expected);
        // Sanity: fallback must not pick up anything from cwd.
        assert!(!resolved.starts_with(cwd.path()));

        restore_config_env(prev);
    }

    #[test]
    #[serial]
    fn config_dir_errors_when_env_dir_missing() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let missing = tmp.path().join("does-not-exist");
        let var = env_var_name();
        let prev = env::var(&var).ok();
        // SAFETY: serialized across tests via `#[serial]`.
        unsafe { env::set_var(&var, &missing) };

        let result = config_dir();

        // SAFETY: serialized across tests via `#[serial]`.
        unsafe {
            match prev {
                Some(v) => env::set_var(&var, v),
                None => env::remove_var(&var),
            }
        }

        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn load_direct_connection() {
        let env = ConfigDirEnv::new();
        env.write_config(
            r#"
dbt_api_connection:
  type: direct
  dbt_api_url: https://api.example.com
  dbt_api_token: secret-token
manifest_storage:
  type: local
  path: /var/manifest
service_account_path: /sa.json
project: my-project
"#,
        );
        let config = load_config().expect("load config");

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
    #[serial]
    fn load_gcp_function_proxy_connection() {
        let env = ConfigDirEnv::new();
        env.write_config(
            r#"
dbt_api_connection:
  type: gcp_function_proxy
  endpoint_url: https://gcp.example.com/fn
  auth_with_service_account: true
manifest_storage:
  type: local
  path: /var/manifest
"#,
        );
        let config = load_config().expect("load config");

        match config.dbt_api_connection {
            DbtApiConnection::GcpFunctionProxy { endpoint_url, auth_with_service_account } => {
                assert_eq!(endpoint_url, "https://gcp.example.com/fn");
                assert!(auth_with_service_account);
            }
            _ => panic!("expected GcpFunctionProxy variant"),
        }
    }

    #[test]
    #[serial]
    fn load_normal_proxy_connection_without_token() {
        let env = ConfigDirEnv::new();
        env.write_config(
            r#"
dbt_api_connection:
  type: normal_proxy
  proxy_url: https://proxy.example.com
manifest_storage:
  type: local
  path: /var/manifest
"#,
        );
        let config = load_config().expect("load config");

        match config.dbt_api_connection {
            DbtApiConnection::NormalProxy { proxy_url, proxy_token } => {
                assert_eq!(proxy_url, "https://proxy.example.com");
                assert!(proxy_token.is_none(), "proxy_token should default to None when omitted");
            }
            _ => panic!("expected NormalProxy variant"),
        }
    }

    #[test]
    #[serial]
    fn load_local_manifest_storage() {
        let env = ConfigDirEnv::new();
        env.write_config(
            r#"
dbt_api_connection:
  type: direct
  dbt_api_url: https://api.example.com
  dbt_api_token: tok
manifest_storage:
  type: local
  path: /var/manifest
"#,
        );
        let config = load_config().expect("load config");

        match config.manifest_storage {
            ManifestStorage::Local { path } => assert_eq!(path, "/var/manifest"),
            _ => panic!("expected Local variant"),
        }
    }

    #[test]
    #[serial]
    fn load_gcs_manifest_storage() {
        let env = ConfigDirEnv::new();
        env.write_config(
            r#"
dbt_api_connection:
  type: direct
  dbt_api_url: https://api.example.com
  dbt_api_token: tok
manifest_storage:
  type: gcs
  bucket: my-bucket
  path: prefix/manifest
  test_file: prefix/.healthcheck
"#,
        );
        let config = load_config().expect("load config");

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
    #[serial]
    fn load_without_service_account_path() {
        let env = ConfigDirEnv::new();
        env.write_config(
            r#"
dbt_api_connection:
  type: direct
  dbt_api_url: https://api.example.com
  dbt_api_token: tok
manifest_storage:
  type: local
  path: /var/manifest
"#,
        );
        let config = load_config().expect("load config");
        assert!(config.service_account_path.is_none());
    }

    #[test]
    #[serial]
    fn load_errors_on_malformed_yaml() {
        let env = ConfigDirEnv::new();
        env.write_config("::: not yaml :::");
        assert!(load_config().is_err());
    }

    #[test]
    #[serial]
    fn load_errors_on_unknown_connection_type() {
        let env = ConfigDirEnv::new();
        env.write_config(
            r#"
dbt_api_connection:
  type: bogus
  some_field: value
manifest_storage:
  type: local
  path: /var/manifest
"#,
        );
        assert!(load_config().is_err());
    }

    #[test]
    #[serial]
    fn save_then_load_roundtrips() {
        let env = ConfigDirEnv::new();
        let original = AppConfig {
            dbt_api_connection: DbtApiConnection::Direct {
                dbt_api_url: "https://api.example.com".to_string(),
                dbt_api_token: "tok".to_string(),
            },
            manifest_storage: ManifestStorage::Local {
                path: "/var/manifest".to_string(),
            },
            service_account_path: Some("/sa.json".to_string()),
            project: Some("my-project".to_string()),
        };

        save_config(&original).expect("save config");
        assert!(env.path().join("config.yaml").exists());

        let loaded = load_config().expect("load config");
        match loaded.dbt_api_connection {
            DbtApiConnection::Direct { dbt_api_url, dbt_api_token } => {
                assert_eq!(dbt_api_url, "https://api.example.com");
                assert_eq!(dbt_api_token, "tok");
            }
            _ => panic!("expected Direct variant"),
        }
        match loaded.manifest_storage {
            ManifestStorage::Local { path } => assert_eq!(path, "/var/manifest"),
            _ => panic!("expected Local variant"),
        }
        assert_eq!(loaded.service_account_path.as_deref(), Some("/sa.json"));
    }
}

