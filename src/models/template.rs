use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use include_dir::{Dir, include_dir};
use minijinja::{AutoEscape, Environment, UndefinedBehavior};
use regex::Regex;

use crate::models::config::{ConfigScope, config_dir};

/// Predefined templates bundled into the binary at compile time from the
/// repo-root `templates/` directory.
static PREDEFINED: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Where a template lives, in precedence order predefined > user > project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateSource {
    /// Bundled into the binary; immutable.
    Predefined,
    /// Global config directory under `templates/`.
    User,
    /// Current project under `.templates/`.
    Project,
}

impl std::fmt::Display for TemplateSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateSource::Predefined => write!(f, "predefined"),
            TemplateSource::User => write!(f, "user"),
            TemplateSource::Project => write!(f, "project"),
        }
    }
}

/// A discovered template file: its source, name, raw file contents, and on-disk
/// path (`None` for bundled/predefined templates).
#[derive(Debug, Clone)]
pub struct TemplateEntry {
    pub source: TemplateSource,
    pub name: String,
    pub raw: String,
    pub path: Option<PathBuf>,
}

/// A template split into its custom-tag parts and the renderable body.
#[derive(Debug, Clone)]
pub struct ParsedTemplate {
    /// Verbatim content of the `{% docs %}…{% enddocs %}` block, trimmed.
    pub docs: Option<String>,
    /// The raw (pre-interpolation) `{% output '…' %}` path expression.
    pub output: Option<String>,
    /// The remaining template, with both custom tags stripped and trimmed.
    pub body: String,
}

/// All sources, in precedence order.
pub const ALL_SOURCES: [TemplateSource; 3] = [
    TemplateSource::Predefined,
    TemplateSource::User,
    TemplateSource::Project,
];

/// True if `ext` (lowercased) is a Jinja extension we recognize.
fn is_jinja_ext(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "jinja" | "j2" | "jinja2")
}

/// User templates directory: `<global config dir>/templates`.
pub fn user_templates_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let (dir, _) = config_dir(Some(ConfigScope::Global))?;
    Ok(dir.join("templates"))
}

/// Project templates directory for `cwd`: `<cwd>/.templates`.
pub fn project_templates_dir(cwd: &Path) -> PathBuf {
    cwd.join(".templates")
}

/// Read the bundled predefined templates.
fn read_predefined() -> Vec<TemplateEntry> {
    let mut entries = Vec::new();
    for file in PREDEFINED.files() {
        let path = file.path();
        let is_jinja = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_jinja_ext);
        if !is_jinja {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(raw) = file.contents_utf8() else {
            continue;
        };
        entries.push(TemplateEntry {
            source: TemplateSource::Predefined,
            name: name.to_string(),
            raw: raw.to_string(),
            path: None,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Read every `*.jinja`/`*.j2`/`*.jinja2` file in `dir` as a template from
/// `source`. A missing directory (or any read error) yields an empty list.
pub fn read_templates_from_dir(dir: &Path, source: TemplateSource) -> Vec<TemplateEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_jinja = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_jinja_ext);
        if !is_jinja {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        entries.push(TemplateEntry {
            source,
            name: name.to_string(),
            raw,
            path: Some(path),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Gather templates from the requested `sources`, concatenated in the canonical
/// predefined → user → project precedence order.
pub fn list_templates(
    sources: &[TemplateSource],
    cwd: &Path,
) -> Result<Vec<TemplateEntry>, Box<dyn std::error::Error>> {
    let mut entries = Vec::new();
    if sources.contains(&TemplateSource::Predefined) {
        entries.extend(read_predefined());
    }
    if sources.contains(&TemplateSource::User) {
        let dir = user_templates_dir()?;
        entries.extend(read_templates_from_dir(&dir, TemplateSource::User));
    }
    if sources.contains(&TemplateSource::Project) {
        let dir = project_templates_dir(cwd);
        entries.extend(read_templates_from_dir(&dir, TemplateSource::Project));
    }
    Ok(entries)
}

/// Case-insensitive lookup of every entry matching `name`.
pub fn find_by_name<'a>(entries: &'a [TemplateEntry], name: &str) -> Vec<&'a TemplateEntry> {
    entries
        .iter()
        .filter(|e| e.name.eq_ignore_ascii_case(name))
        .collect()
}

/// Validate that `name` is usable as a single template name: non-empty and free
/// of path separators or extension dots.
pub fn validate_template_name(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    if name.trim().is_empty() {
        return Err("template name must not be empty".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains('.') {
        return Err("template name must not contain '/', '\\', or '.'".into());
    }
    Ok(())
}

// Custom-tag matchers. These are dbt-assist's own extensions on top of Jinja;
// they are expected to live *outside* any `{% raw %}` block (the body strip is
// global, so a custom tag buried inside a raw block would also be removed).
static OUTPUT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)\{%-?\s*output\s+(?:'([^']*)'|"([^"]*)")\s*-?%\}"#).unwrap()
});
static DOCS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\{%-?\s*docs\s*-?%\}(.*?)\{%-?\s*enddocs\s*-?%\}").unwrap());
static DOCS_OPEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{%-?\s*docs\s*-?%\}").unwrap());

/// Split a template into its `{% docs %}` block, `{% output %}` path expression,
/// and the remaining renderable body.
pub fn parse_template(raw: &str) -> Result<ParsedTemplate, Box<dyn std::error::Error>> {
    let outputs: Vec<_> = OUTPUT_RE.captures_iter(raw).collect();
    if outputs.len() > 1 {
        return Err("template defines more than one {% output %} tag".into());
    }
    let output = outputs.first().map(|c| {
        // Either the single-quoted (group 1) or double-quoted (group 2) capture.
        c.get(1)
            .or_else(|| c.get(2))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default()
    });

    let docs_matches: Vec<_> = DOCS_RE.captures_iter(raw).collect();
    if docs_matches.len() > 1 {
        return Err("template defines more than one {% docs %} block".into());
    }
    let docs = match docs_matches.first() {
        Some(c) => Some(c[1].trim().to_string()),
        None => {
            if DOCS_OPEN_RE.is_match(raw) {
                return Err("{% docs %} block is not closed with {% enddocs %}".into());
            }
            None
        }
    };

    let stripped = DOCS_RE.replace_all(raw, "");
    let stripped = OUTPUT_RE.replace_all(&stripped, "");
    let body = stripped.trim().to_string();

    Ok(ParsedTemplate { docs, output, body })
}

/// Render a Jinja `source` string with `vars`. Undefined variables are an error
/// (strict), so the user must supply every variable the template uses; unused
/// extra variables are ignored, and no HTML escaping is applied. Content inside
/// `{% raw %}` blocks is never evaluated, so dbt's own `{{ … }}` is unaffected.
pub fn render_str(
    source: &str,
    vars: &BTreeMap<String, String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::None);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.render_str(source, vars)
        .map_err(|e| format!("template render failed: {e}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn read_templates_parses_stem_and_accepts_jinja_extensions() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("proxy.jinja"), "body").unwrap();
        std::fs::write(tmp.path().join("seed.j2"), "body").unwrap();
        std::fs::write(tmp.path().join("view.jinja2"), "body").unwrap();
        // Non-Jinja files are ignored.
        std::fs::write(tmp.path().join("notes.txt"), "ignore me").unwrap();

        let entries = read_templates_from_dir(tmp.path(), TemplateSource::Project);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["proxy", "seed", "view"]);
        assert!(entries.iter().all(|e| e.source == TemplateSource::Project));
        assert!(entries.iter().all(|e| e.path.is_some()));
    }

    #[test]
    fn read_templates_missing_dir_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(read_templates_from_dir(&missing, TemplateSource::User).is_empty());
    }

    #[test]
    fn find_by_name_is_case_insensitive() {
        let entries = vec![TemplateEntry {
            source: TemplateSource::Project,
            name: "Proxy".to_string(),
            raw: String::new(),
            path: None,
        }];
        assert_eq!(find_by_name(&entries, "proxy").len(), 1);
        assert_eq!(find_by_name(&entries, "PROXY").len(), 1);
        assert_eq!(find_by_name(&entries, "seed").len(), 0);
    }

    #[test]
    fn validate_template_name_rejects_bad_names() {
        assert!(validate_template_name("proxy").is_ok());
        assert!(validate_template_name("").is_err());
        assert!(validate_template_name("   ").is_err());
        assert!(validate_template_name("a/b").is_err());
        assert!(validate_template_name("a\\b").is_err());
        assert!(validate_template_name("a.jinja").is_err());
    }

    #[test]
    fn parse_output_single_and_double_quotes() {
        let single = parse_template("{% output 'models/x.sql' %}\nbody").unwrap();
        assert_eq!(single.output.as_deref(), Some("models/x.sql"));
        let double = parse_template("{% output \"models/x.sql\" %}\nbody").unwrap();
        assert_eq!(double.output.as_deref(), Some("models/x.sql"));
    }

    #[test]
    fn parse_output_keeps_interpolation_raw() {
        let parsed =
            parse_template("{% output 'models/{{dataset}}/{{table}}.sql' %}\nbody").unwrap();
        assert_eq!(
            parsed.output.as_deref(),
            Some("models/{{dataset}}/{{table}}.sql")
        );
    }

    #[test]
    fn parse_accepts_trim_markers() {
        let parsed =
            parse_template("{%- output 'x.sql' -%}\n{%- docs -%}d{%- enddocs -%}\nbody").unwrap();
        assert_eq!(parsed.output.as_deref(), Some("x.sql"));
        assert_eq!(parsed.docs.as_deref(), Some("d"));
    }

    #[test]
    fn parse_two_output_tags_is_error() {
        let raw = "{% output 'a.sql' %}\n{% output 'b.sql' %}\nbody";
        assert!(parse_template(raw).is_err());
    }

    #[test]
    fn parse_docs_extracted_and_trimmed_dotall() {
        let raw = "{% docs %}\nline one\nline two\n{% enddocs %}\nbody";
        let parsed = parse_template(raw).unwrap();
        assert_eq!(parsed.docs.as_deref(), Some("line one\nline two"));
    }

    #[test]
    fn parse_unclosed_docs_is_error() {
        assert!(parse_template("{% docs %}\nhello\nbody").is_err());
    }

    #[test]
    fn parse_two_docs_blocks_is_error() {
        let raw = "{% docs %}a{% enddocs %}\n{% docs %}b{% enddocs %}\nbody";
        assert!(parse_template(raw).is_err());
    }

    #[test]
    fn parse_no_tags_body_is_trimmed_raw() {
        let parsed = parse_template("\n  just a body  \n").unwrap();
        assert_eq!(parsed.docs, None);
        assert_eq!(parsed.output, None);
        assert_eq!(parsed.body, "just a body");
    }

    #[test]
    fn parse_raw_block_survives_into_body() {
        let raw = "{% output 'x.sql' %}\n{% raw %}{{ literal }}{% endraw %}";
        let parsed = parse_template(raw).unwrap();
        assert_eq!(parsed.body, "{% raw %}{{ literal }}{% endraw %}");
    }

    #[test]
    fn render_missing_var_errors() {
        // Strict mode: using an undefined variable is an error.
        assert!(render_str("a={{ a }};b={{ b }}", &vars(&[("a", "1")])).is_err());
    }

    #[test]
    fn render_ignores_unused_var() {
        // Supplying an extra variable the template never uses is fine.
        let out = render_str("hi {{ name }}", &vars(&[("name", "x"), ("unused", "y")])).unwrap();
        assert_eq!(out, "hi x");
    }

    #[test]
    fn render_raw_block_is_not_evaluated() {
        // dbt's own Jinja inside {% raw %} must not trigger strict undefined errors.
        let out = render_str("{% raw %}{{ source('a', 'b') }}{% endraw %}", &vars(&[])).unwrap();
        assert_eq!(out, "{{ source('a', 'b') }}");
    }

    #[test]
    fn render_output_path_template() {
        let out = render_str(
            "models/proxies/{{dataset}}/{{dataset}}_{{table}}.sql",
            &vars(&[("dataset", "x"), ("table", "y")]),
        )
        .unwrap();
        assert_eq!(out, "models/proxies/x/x_y.sql");
    }
}
