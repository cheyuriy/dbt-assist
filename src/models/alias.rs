use std::path::{Path, PathBuf};

use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};

use crate::errors::{EnvironmentError, ValidationError};
use crate::models::config::{ConfigScope, config_dir};

/// Predefined aliases bundled into the binary at compile time from the
/// repo-root `aliases/` directory.
static PREDEFINED: Dir = include_dir!("$CARGO_MANIFEST_DIR/aliases");

/// A parsed alias definition: the parameters later fed to `dbt build`.
///
/// Serialized as a small YAML file whose name (sans extension) is the alias
/// name. `exclude`/`full_refresh` are omitted from the file when unset — an
/// absent `full_refresh` is *not* the same as `false`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    pub select: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exclude: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub full_refresh: Option<bool>,
}

/// Where an alias lives, in precedence order predefined > user > project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasSource {
    /// Bundled into the binary; immutable.
    Predefined,
    /// Global config directory under `aliases/`.
    User,
    /// Current project under `.aliases/`.
    Project,
}

impl std::fmt::Display for AliasSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AliasSource::Predefined => write!(f, "predefined"),
            AliasSource::User => write!(f, "user"),
            AliasSource::Project => write!(f, "project"),
        }
    }
}

/// A discovered alias file: its source, name, raw YAML contents, and on-disk
/// path (`None` for bundled/predefined aliases).
#[derive(Debug, Clone)]
pub struct AliasEntry {
    pub source: AliasSource,
    pub name: String,
    pub definition: String,
    pub path: Option<PathBuf>,
}

/// True if `ext` (lowercased) is a YAML extension we recognize.
fn is_yaml_ext(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "yml" | "yaml")
}

/// User aliases directory: `<global config dir>/aliases`.
pub fn user_aliases_dir() -> Result<PathBuf, EnvironmentError> {
    let (dir, _) = config_dir(Some(ConfigScope::Global))?;
    Ok(dir.join("aliases"))
}

/// Project aliases directory for `cwd`: `<cwd>/.aliases`.
pub fn project_aliases_dir(cwd: &Path) -> PathBuf {
    cwd.join(".aliases")
}

/// Read the bundled predefined aliases.
fn read_predefined() -> Vec<AliasEntry> {
    let mut entries = Vec::new();
    for file in PREDEFINED.files() {
        let path = file.path();
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_yaml_ext);
        if !is_yaml {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(definition) = file.contents_utf8() else {
            continue;
        };
        entries.push(AliasEntry {
            source: AliasSource::Predefined,
            name: name.to_string(),
            definition: definition.to_string(),
            path: None,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Read every `*.yml`/`*.yaml` file in `dir` as an alias from `source`. A
/// missing directory (or any read error) yields an empty list.
pub fn read_aliases_from_dir(dir: &Path, source: AliasSource) -> Vec<AliasEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_yaml_ext);
        if !is_yaml {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(definition) = std::fs::read_to_string(&path) else {
            continue;
        };
        entries.push(AliasEntry {
            source,
            name: name.to_string(),
            definition,
            path: Some(path),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Gather aliases from the requested `sources`, concatenated in the canonical
/// predefined → user → project precedence order.
pub fn list_aliases(
    sources: &[AliasSource],
    cwd: &Path,
) -> Result<Vec<AliasEntry>, EnvironmentError> {
    let mut entries = Vec::new();
    if sources.contains(&AliasSource::Predefined) {
        entries.extend(read_predefined());
    }
    if sources.contains(&AliasSource::User) {
        let dir = user_aliases_dir()?;
        entries.extend(read_aliases_from_dir(&dir, AliasSource::User));
    }
    if sources.contains(&AliasSource::Project) {
        let dir = project_aliases_dir(cwd);
        entries.extend(read_aliases_from_dir(&dir, AliasSource::Project));
    }
    Ok(entries)
}

/// All sources, in precedence order.
pub const ALL_SOURCES: [AliasSource; 3] = [
    AliasSource::Predefined,
    AliasSource::User,
    AliasSource::Project,
];

/// Case-insensitive lookup of every entry matching `name`.
pub fn find_by_name<'a>(entries: &'a [AliasEntry], name: &str) -> Vec<&'a AliasEntry> {
    entries
        .iter()
        .filter(|e| e.name.eq_ignore_ascii_case(name))
        .collect()
}

/// Validate that `name` is usable as a single alias filename: non-empty and
/// free of path separators or extension dots.
pub fn validate_alias_name(name: &str) -> Result<(), ValidationError> {
    if name.trim().is_empty() {
        return Err(ValidationError::EmptyName { kind: "alias" });
    }
    if name.contains('/') || name.contains('\\') || name.contains('.') {
        return Err(ValidationError::IllegalChars { kind: "alias" });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_omits_unset_optional_fields() {
        let alias = Alias {
            select: "*".to_string(),
            exclude: None,
            full_refresh: None,
        };
        let yaml = serde_yml::to_string(&alias).unwrap();
        assert!(yaml.contains("select"));
        assert!(!yaml.contains("exclude"));
        assert!(!yaml.contains("full_refresh"));
    }

    #[test]
    fn alias_writes_full_refresh_false() {
        let alias = Alias {
            select: "tag:daily".to_string(),
            exclude: Some("tag:wip".to_string()),
            full_refresh: Some(false),
        };
        let yaml = serde_yml::to_string(&alias).unwrap();
        assert!(yaml.contains("exclude"));
        assert!(yaml.contains("full_refresh"));
        // Round-trips back to the same value.
        let parsed: Alias = serde_yml::from_str(&yaml).unwrap();
        assert_eq!(parsed, alias);
    }

    #[test]
    fn read_aliases_parses_stem_and_accepts_both_extensions() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("daily.yml"), "select: \"tag:daily\"\n").unwrap();
        std::fs::write(tmp.path().join("weekly.yaml"), "select: \"tag:weekly\"\n").unwrap();
        // Non-YAML files are ignored.
        std::fs::write(tmp.path().join("notes.txt"), "ignore me").unwrap();

        let entries = read_aliases_from_dir(tmp.path(), AliasSource::Project);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["daily", "weekly"]);
        assert!(entries.iter().all(|e| e.source == AliasSource::Project));
        assert!(entries.iter().all(|e| e.path.is_some()));
    }

    #[test]
    fn read_aliases_missing_dir_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(read_aliases_from_dir(&missing, AliasSource::User).is_empty());
    }

    #[test]
    fn find_by_name_is_case_insensitive() {
        let entries = vec![AliasEntry {
            source: AliasSource::Project,
            name: "Daily".to_string(),
            definition: String::new(),
            path: None,
        }];
        assert_eq!(find_by_name(&entries, "daily").len(), 1);
        assert_eq!(find_by_name(&entries, "DAILY").len(), 1);
        assert_eq!(find_by_name(&entries, "weekly").len(), 0);
    }

    #[test]
    fn validate_alias_name_rejects_bad_names() {
        assert!(validate_alias_name("daily").is_ok());
        assert!(validate_alias_name("").is_err());
        assert!(validate_alias_name("   ").is_err());
        assert!(validate_alias_name("a/b").is_err());
        assert!(validate_alias_name("a\\b").is_err());
        assert!(validate_alias_name("a.yml").is_err());
    }
}
