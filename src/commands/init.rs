use std::fs;
use std::path::{Path, PathBuf};

use colored::Colorize;
use dialoguer::Confirm;
use serde_json::{Value, json};

use crate::vprintln;

/// Initialize the current dbt project: scaffold the hidden working directories
/// dbt-assist relies on and, optionally, wire up local VSCode settings for the
/// "Power User for dbt" extension (deferral, lineage panel, jinja associations).
pub fn init() {
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

    if !crate::util::is_dbt_project(&cwd) {
        eprintln!(
            "{} `init` must be run from a dbt project directory (no {} found here).",
            "error:".red().bold(),
            "dbt_project.yml".bold()
        );
        return;
    }

    // Scaffold the hidden working directories.
    for (name, purpose) in [
        (".manifest", "to store manifest.json"),
        (".aliases", "to store dbt command aliases"),
        (".templates", "to store dbt model templates"),
    ] {
        let dir = cwd.join(name);
        match fs::create_dir_all(&dir) {
            Ok(()) => vprintln!("Created {} ({})", dir.display(), purpose),
            Err(e) => {
                eprintln!(
                    "{} Could not create {}: {e}",
                    "error:".red().bold(),
                    dir.display()
                );
                return;
            }
        }
    }

    let configure = Confirm::new()
        .with_prompt("Create/update local VSCode settings for dbt deferral?")
        .default(true)
        .interact()
        .unwrap_or(false);

    if configure {
        configure_vscode_settings(&cwd);
    }

    let update_gitignore = Confirm::new()
        .with_prompt("Add the created folders to .gitignore?")
        .default(true)
        .interact()
        .unwrap_or(false);

    if update_gitignore {
        configure_gitignore(&cwd);
    }

    println!(
        "{} Initialization complete. Run {} to update state for the \"defer\" function.",
        "✓".green().bold(),
        "dbt-assist manifest".bold()
    );
}

/// Create or patch `.vscode/settings.json`, preserving any existing settings.
/// If an existing file can't be parsed as strict JSON (e.g. it contains JSONC
/// comments), we warn and leave it untouched rather than risk clobbering it.
fn configure_vscode_settings(cwd: &Path) {
    let settings_path = cwd.join(".vscode").join("settings.json");

    let mut root: Value = if settings_path.exists() {
        let contents = match fs::read_to_string(&settings_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "{} Could not read {}: {e}",
                    "error:".red().bold(),
                    settings_path.display()
                );
                return;
            }
        };
        match serde_json::from_str(&contents) {
            Ok(value) => value,
            Err(e) => {
                println!(
                    "{} Could not parse {} as JSON ({e}). It may contain comments or \
                     trailing commas; please add the dbt-assist settings manually.",
                    "warning:".yellow().bold(),
                    settings_path.display()
                );
                return;
            }
        }
    } else {
        json!({})
    };

    if !root.is_object() {
        println!(
            "{} {} is not a JSON object; leaving it untouched.",
            "warning:".yellow().bold(),
            settings_path.display()
        );
        return;
    }

    let manifest_path = cwd.join(".manifest").join("manifest.json");
    apply_vscode_settings(&mut root, &manifest_path.to_string_lossy());

    // Detect (but don't configure) nested dbt sub-projects.
    let sub_projects = find_sub_projects(cwd);
    if !sub_projects.is_empty() {
        println!(
            "{} Found {} nested dbt sub-project(s). Setting these up is not supported yet; \
             configure them manually in {}:",
            "note:".yellow().bold(),
            sub_projects.len(),
            ".vscode/settings.json".bold()
        );
        for sp in &sub_projects {
            println!("  - {}", sp.display());
        }
    }

    if let Some(parent) = settings_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        eprintln!(
            "{} Could not create {}: {e}",
            "error:".red().bold(),
            parent.display()
        );
        return;
    }

    let serialized = match serde_json::to_string_pretty(&root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{} Could not serialize settings: {e}",
                "error:".red().bold()
            );
            return;
        }
    };
    match fs::write(&settings_path, serialized) {
        Ok(()) => vprintln!("Wrote {}", settings_path.display()),
        Err(e) => eprintln!(
            "{} Could not write {}: {e}",
            "error:".red().bold(),
            settings_path.display()
        ),
    }
}

/// Folders dbt-assist creates that should not be committed.
const GITIGNORE_ENTRIES: [&str; 4] = [".aliases", ".manifest", ".templates", ".vscode"];

/// Append dbt-assist's created folders to `.gitignore` (creating it if needed),
/// skipping any entry that is already listed.
fn configure_gitignore(cwd: &Path) {
    let gitignore_path = cwd.join(".gitignore");

    let existing = match fs::read_to_string(&gitignore_path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            eprintln!(
                "{} Could not read {}: {e}",
                "error:".red().bold(),
                gitignore_path.display()
            );
            return;
        }
    };

    let already: std::collections::HashSet<&str> =
        existing.lines().map(|l| l.trim()).collect();
    let missing: Vec<&str> = GITIGNORE_ENTRIES
        .iter()
        .copied()
        .filter(|e| !already.contains(e))
        .collect();

    if missing.is_empty() {
        vprintln!("All folders already present in {}", gitignore_path.display());
        return;
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    for entry in &missing {
        updated.push_str(entry);
        updated.push('\n');
    }

    match fs::write(&gitignore_path, updated) {
        Ok(()) => vprintln!(
            "Added {} to {}",
            missing.join(", "),
            gitignore_path.display()
        ),
        Err(e) => eprintln!(
            "{} Could not write {}: {e}",
            "error:".red().bold(),
            gitignore_path.display()
        ),
    }
}

/// Apply dbt-assist's managed keys to a VSCode settings object, leaving all
/// other keys intact. `root` must be a JSON object.
fn apply_vscode_settings(root: &mut Value, manifest_path: &str) {
    let obj = root
        .as_object_mut()
        .expect("apply_vscode_settings called on non-object");

    obj.insert(
        "dbt.deferConfigPerProject".to_string(),
        json!({
            "": {
                "deferToProduction": true,
                "manifestPathForDeferral": manifest_path,
                "manifestPathType": "local",
                "favorState": false
            }
        }),
    );

    obj.insert("dbt.enableNewLineagePanel".to_string(), json!(true));

    let associations = obj.entry("files.associations").or_insert_with(|| json!({}));
    if !associations.is_object() {
        *associations = json!({});
    }
    let associations = associations.as_object_mut().unwrap();
    associations.insert("*.sql".to_string(), json!("jinja-sql"));
    associations.insert("*.yml".to_string(), json!("jinja-yml"));
}

/// Recursively search `root`'s subdirectories for nested dbt projects
/// (directories containing `dbt_project.yml`), skipping hidden directories.
/// `root` itself is never reported.
fn find_sub_projects(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect_sub_projects(root, root, &mut found);
    found
}

fn collect_sub_projects(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Skip hidden directories (includes .manifest/.aliases/.templates/.vscode).
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'))
        {
            continue;
        }
        if crate::util::is_dbt_project(&path) && path != root {
            found.push(path.clone());
        }
        collect_sub_projects(root, &path, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_vscode_settings_sets_all_keys_on_empty_object() {
        let mut root = json!({});
        apply_vscode_settings(&mut root, "/abs/.manifest/manifest.json");

        let defer = &root["dbt.deferConfigPerProject"][""];
        assert_eq!(defer["deferToProduction"], json!(true));
        assert_eq!(
            defer["manifestPathForDeferral"],
            json!("/abs/.manifest/manifest.json")
        );
        assert_eq!(defer["manifestPathType"], json!("local"));
        assert_eq!(defer["favorState"], json!(false));

        assert_eq!(root["dbt.enableNewLineagePanel"], json!(true));
        assert_eq!(root["files.associations"]["*.sql"], json!("jinja-sql"));
        assert_eq!(root["files.associations"]["*.yml"], json!("jinja-yml"));
    }

    #[test]
    fn apply_vscode_settings_preserves_existing_settings() {
        let mut root = json!({
            "editor.formatOnSave": true,
            "files.associations": { "*.md": "markdown" }
        });
        apply_vscode_settings(&mut root, "/abs/.manifest/manifest.json");

        // Unrelated key untouched.
        assert_eq!(root["editor.formatOnSave"], json!(true));
        // Existing association preserved, new ones added.
        assert_eq!(root["files.associations"]["*.md"], json!("markdown"));
        assert_eq!(root["files.associations"]["*.sql"], json!("jinja-sql"));
        assert_eq!(root["files.associations"]["*.yml"], json!("jinja-yml"));
    }

    #[test]
    fn find_sub_projects_finds_nested_and_skips_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Root is a dbt project.
        fs::write(root.join("dbt_project.yml"), "name: root\n").unwrap();

        // Nested project under a normal subdirectory.
        let nested = root.join("subdir").join("nested_project");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("dbt_project.yml"), "name: nested\n").unwrap();

        // Project under a hidden directory should be ignored.
        let hidden = root.join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        fs::write(hidden.join("dbt_project.yml"), "name: hidden\n").unwrap();

        let found = find_sub_projects(root);
        assert_eq!(found, vec![nested]);
    }

    #[test]
    fn configure_gitignore_creates_file_with_all_entries() {
        let tmp = tempfile::tempdir().unwrap();
        configure_gitignore(tmp.path());
        let contents = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        for entry in GITIGNORE_ENTRIES {
            assert!(contents.lines().any(|l| l.trim() == entry), "missing {entry}");
        }
    }

    #[test]
    fn configure_gitignore_appends_only_missing_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".gitignore");
        // Pre-existing content with one of our entries already present (no trailing newline).
        fs::write(&path, "target/\n.manifest").unwrap();
        configure_gitignore(tmp.path());

        let contents = fs::read_to_string(&path).unwrap();
        // Unrelated entry preserved.
        assert!(contents.lines().any(|l| l.trim() == "target/"));
        // `.manifest` not duplicated.
        assert_eq!(contents.lines().filter(|l| l.trim() == ".manifest").count(), 1);
        // The other three were appended.
        for entry in [".aliases", ".templates", ".vscode"] {
            assert!(contents.lines().any(|l| l.trim() == entry), "missing {entry}");
        }
    }
}
