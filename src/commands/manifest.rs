use std::fs;
use std::path::Path;
use std::time::SystemTime;

use colored::Colorize;

use crate::models::config::{ConfigScope, ManifestStorage, load_config};
use crate::vprintln;

/// Refresh the local `manifest.json` so dbt's `defer` function has up-to-date
/// production state to compare against. Pulls the manifest from wherever the
/// config says it lives (a local directory or a GCS bucket), copies it into the
/// project's `.manifest` directory (or `--manifest-dir`), and warns when the
/// source manifest looks stale.
pub fn manifest(
    scope: Option<ConfigScope>,
    project_name: Option<String>,
    manifest_dir: Option<String>,
) {
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!(
                "{} Could not resolve current directory: {e}",
                "error:".red().bold()
            );
            return;
        }
    };

    if !crate::utils::is_dbt_project(&cwd) {
        eprintln!(
            "{} `manifest` must be run from a dbt project directory (no {} found here).",
            "error:".red().bold(),
            "dbt_project.yml".bold()
        );
        return;
    }

    let (config, resolved) = match load_config(scope) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("{} Could not load config: {e}", "error:".red().bold());
            return;
        }
    };
    vprintln!("Loaded {resolved} config");

    // Resolve the destination directory and ensure it exists.
    let dest_dir = cwd.join(manifest_dir.as_deref().unwrap_or(".manifest"));
    if let Err(e) = fs::create_dir_all(&dest_dir) {
        eprintln!(
            "{} Could not create {}: {e}",
            "error:".red().bold(),
            dest_dir.display()
        );
        return;
    }
    let dest = dest_dir.join("manifest.json");

    let source_modified = match &config.manifest_storage {
        ManifestStorage::Local { path } => match copy_local(path, &dest) {
            Ok(modified) => modified,
            Err(e) => {
                eprintln!("{} {e}", "error:".red().bold());
                return;
            }
        },
        ManifestStorage::GCS { bucket, path, .. } => {
            let project = match resolve_project_name(project_name.as_deref(), &cwd) {
                Ok(name) => name,
                Err(e) => {
                    eprintln!("{} {e}", "error:".red().bold());
                    return;
                }
            };
            let object = manifest_object_key(path, &project);
            vprintln!("Downloading gs://{bucket}/{object}");
            match download_gcs(&config, bucket, &object, &dest) {
                Ok(modified) => modified,
                Err(e) => {
                    eprintln!("{} {e}", "error:".red().bold());
                    return;
                }
            }
        }
    };

    vprintln!("Wrote {}", dest.display());
    report_age(source_modified);

    println!(
        "{} Manifest refreshed. State for the \"defer\" function is updated.",
        "✓".green().bold()
    );
}

/// Copies `<path>/manifest.json` to `dest`, returning the source's last-modified
/// time (captured before the copy) so its age can be reported.
fn copy_local(path: &str, dest: &Path) -> Result<Option<SystemTime>, Box<dyn std::error::Error>> {
    let src = crate::utils::expand_tilde(path).join("manifest.json");
    if !src.is_file() {
        return Err(format!("Manifest not found at {}", src.display()).into());
    }
    let modified = fs::metadata(&src)?.modified().ok();
    fs::copy(&src, dest)?;
    Ok(modified)
}

/// Downloads the manifest object from GCS and writes it to `dest`, returning the
/// object's GCS last-update time (so the local copy's fresh mtime doesn't mask a
/// stale upstream manifest).
fn download_gcs(
    config: &crate::models::config::AppConfig,
    bucket: &str,
    object: &str,
    dest: &Path,
) -> Result<Option<SystemTime>, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (bytes, updated) =
        rt.block_on(crate::gcp::client::download_manifest(config, bucket, object))?;
    fs::write(dest, bytes)?;
    Ok(updated)
}

/// Resolves the project name: an explicit `--project-name` wins, otherwise fall
/// back to `dbt_project.yml`'s `name:`.
fn resolve_project_name(
    override_: Option<&str>,
    cwd: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(name) = override_ {
        return Ok(name.to_string());
    }
    crate::utils::read_project_name(cwd).ok_or_else(|| {
        "Could not determine project name; pass --project-name or set `name` in dbt_project.yml"
            .into()
    })
}

/// Builds the GCS object key for a project's manifest: `<path>/<project>/manifest.json`
/// (or `<project>/manifest.json` when `path` is empty), collapsing redundant slashes.
fn manifest_object_key(path: &str, project: &str) -> String {
    let path = path.trim_matches('/');
    let project = project.trim_matches('/');
    if path.is_empty() {
        format!("{project}/manifest.json")
    } else {
        format!("{path}/{project}/manifest.json")
    }
}

/// Whole hours elapsed between `modified` and `now` (0 if `modified` is in the future).
fn age_hours(modified: SystemTime, now: SystemTime) -> u64 {
    now.duration_since(modified).unwrap_or_default().as_secs() / 3600
}

/// Prints the manifest's age in hours and, when older than 24 hours, a staleness
/// warning. Does nothing when no timestamp is available.
fn report_age(modified: Option<SystemTime>) {
    let Some(modified) = modified else {
        return;
    };
    let hours = age_hours(modified, SystemTime::now());
    println!("Manifest is {hours} hour(s) old.");
    if hours >= 24 {
        println!(
            "{} The manifest is over 24 hours old and may be out of date.",
            "warning:".yellow().bold()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn resolve_project_name_prefers_override() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("dbt_project.yml"), "name: from_yaml\n").unwrap();
        assert_eq!(
            resolve_project_name(Some("from_flag"), tmp.path()).unwrap(),
            "from_flag"
        );
    }

    #[test]
    fn resolve_project_name_falls_back_to_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("dbt_project.yml"), "name: from_yaml\n").unwrap();
        assert_eq!(
            resolve_project_name(None, tmp.path()).unwrap(),
            "from_yaml"
        );
    }

    #[test]
    fn resolve_project_name_errors_when_unresolvable() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_project_name(None, tmp.path()).is_err());
    }

    #[test]
    fn manifest_object_key_includes_project_and_filename() {
        assert_eq!(
            manifest_object_key("prefix/manifests", "my_project"),
            "prefix/manifests/my_project/manifest.json"
        );
    }

    #[test]
    fn manifest_object_key_collapses_slashes() {
        assert_eq!(
            manifest_object_key("/prefix/", "/my_project/"),
            "prefix/my_project/manifest.json"
        );
    }

    #[test]
    fn manifest_object_key_handles_empty_path() {
        assert_eq!(
            manifest_object_key("", "my_project"),
            "my_project/manifest.json"
        );
    }

    #[test]
    fn age_hours_computes_whole_hours() {
        let now = SystemTime::now();
        let modified = now - Duration::from_secs(3 * 3600 + 59 * 60);
        assert_eq!(age_hours(modified, now), 3);
    }

    #[test]
    fn age_hours_future_modified_is_zero() {
        let now = SystemTime::now();
        let modified = now + Duration::from_secs(3600);
        assert_eq!(age_hours(modified, now), 0);
    }
}
