use std::collections::BTreeMap;

use colored::Colorize;
use comfy_table::{Cell, Table, presets::UTF8_FULL};
use dialoguer::Confirm;

use crate::models::template::{
    ALL_SOURCES, TemplateEntry, TemplateSource, find_by_name, list_templates, parse_template,
    render_str, validate_template_name,
};
use crate::vprintln;

/// `templates list`: print a table of available templates. When no source flag
/// is set, all three sources are shown in precedence order (predefined > user >
/// project); otherwise only the flagged sources.
pub fn list(
    predefined: bool,
    user: bool,
    project: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let sources: Vec<TemplateSource> = if !predefined && !user && !project {
        ALL_SOURCES.to_vec()
    } else {
        ALL_SOURCES
            .into_iter()
            .filter(|s| match s {
                TemplateSource::Predefined => predefined,
                TemplateSource::User => user,
                TemplateSource::Project => project,
            })
            .collect()
    };

    let cwd = std::env::current_dir()
        .map_err(|e| format!("could not resolve current directory: {e}"))?;

    let entries =
        list_templates(&sources, &cwd).map_err(|e| format!("could not list templates: {e}"))?;

    if entries.is_empty() {
        println!("{}", "No templates found.".dimmed());
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL).set_header(vec![
        Cell::new("Source"),
        Cell::new("Name"),
        Cell::new("Path"),
    ]);

    for entry in &entries {
        let path = match &entry.path {
            Some(p) => p.display().to_string(),
            None => "[bundled]".to_string(),
        };
        table.add_row(vec![
            Cell::new(entry.source.to_string()),
            Cell::new(&entry.name),
            Cell::new(path),
        ]);
    }

    println!("{table}");
    Ok(())
}

/// `templates docs`: show the `{% docs %}` block and the raw `{% output %}`
/// path expression for a template.
pub fn docs(
    name: String,
    source: Option<TemplateSource>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("could not resolve current directory: {e}"))?;

    let entries =
        list_templates(&ALL_SOURCES, &cwd).map_err(|e| format!("could not list templates: {e}"))?;

    // `resolve_one` reports its own error; treat that as a handled early exit.
    let Ok(entry) = resolve_one(&entries, &name, source) else {
        return Ok(());
    };

    let parsed = parse_template(&entry.raw)
        .map_err(|e| format!("could not parse template {}: {e}", name.bold()))?;

    println!("{} ({})", name.bold(), entry.source.to_string().dimmed());
    println!();
    match &parsed.docs {
        Some(docs) => println!("{docs}"),
        None => println!("{}", "(no docs)".dimmed()),
    }
    println!();
    match &parsed.output {
        Some(output) => println!("Output: {}", output.bold()),
        None => println!("Output: {}", "(no output tag)".dimmed()),
    }
    Ok(())
}

/// `templates build`: render a template into a dbt model file.
pub fn build(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let parsed_args = parse_build_args(&args)?;

    validate_template_name(&parsed_args.name)?;

    let cwd = std::env::current_dir()
        .map_err(|e| format!("could not resolve current directory: {e}"))?;

    if !crate::utils::is_dbt_project(&cwd) {
        return Err(format!(
            "{} must be run from a dbt project directory (no {} found here).",
            "templates build".bold(),
            "dbt_project.yml".bold()
        )
        .into());
    }

    let entries =
        list_templates(&ALL_SOURCES, &cwd).map_err(|e| format!("could not list templates: {e}"))?;

    // `resolve_one` reports its own error; treat that as a handled early exit.
    let Ok(entry) = resolve_one(&entries, &parsed_args.name, parsed_args.source) else {
        return Ok(());
    };

    let parsed = parse_template(&entry.raw)
        .map_err(|e| format!("could not parse template {}: {e}", parsed_args.name.bold()))?;

    // Resolve the output path: an explicit --output is used literally; otherwise
    // the {% output %} tag is interpolated with the supplied variables.
    let rel_path = match (&parsed_args.output, &parsed.output) {
        (Some(output), _) => output.clone(),
        (None, Some(tag)) => render_str(tag, &parsed_args.vars)
            .map_err(|e| format!("could not render output path: {e}"))?,
        (None, None) => {
            return Err(format!(
                "template {} has no {} tag; pass {} to set the destination.",
                parsed_args.name.bold(),
                "{% output %}".bold(),
                "--output <path>".bold()
            )
            .into());
        }
    };

    let body = render_str(&parsed.body, &parsed_args.vars)?;

    let dest = cwd.join(&rel_path);

    if dest.exists() {
        let proceed = Confirm::new()
            .with_prompt(format!("{} already exists. Overwrite it?", dest.display()))
            .default(false)
            .interact()
            .unwrap_or(false);
        if !proceed {
            println!("{}", "Model not created.".dimmed());
            return Ok(());
        }
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }

    std::fs::write(&dest, body).map_err(|e| format!("could not write {}: {e}", dest.display()))?;
    vprintln!("Wrote {}", dest.display());

    println!(
        "{} Model created at {}.",
        "✓".green().bold(),
        rel_path.bold()
    );
    Ok(())
}

/// Resolve a single template by name, disambiguating with `source`. Prints a
/// user-facing error and returns `Err(())` on no/ambiguous match.
fn resolve_one<'a>(
    entries: &'a [TemplateEntry],
    name: &str,
    source: Option<TemplateSource>,
) -> Result<&'a TemplateEntry, ()> {
    let matches = find_by_name(entries, name);
    if matches.is_empty() {
        eprintln!(
            "{} no template named {} found.",
            "error:".red().bold(),
            name.bold()
        );
        return Err(());
    }

    if let Some(src) = source {
        return match matches.iter().find(|e| e.source == src) {
            Some(entry) => Ok(entry),
            None => {
                eprintln!(
                    "{} no template named {} found in {}.",
                    "error:".red().bold(),
                    name.bold(),
                    src.to_string().bold()
                );
                Err(())
            }
        };
    }

    if matches.len() > 1 {
        let where_str = matches
            .iter()
            .map(|e| e.source.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "{} template {} exists in multiple sources ({}). pass {} to disambiguate.",
            "error:".red().bold(),
            name.bold(),
            where_str.bold(),
            "--source".bold()
        );
        return Err(());
    }

    Ok(matches[0])
}

/// Parsed `templates build` arguments.
struct BuildArgs {
    name: String,
    source: Option<TemplateSource>,
    output: Option<String>,
    vars: BTreeMap<String, String>,
}

/// Parse the raw trailing args of `templates build`: the first non-flag token is
/// the template name; reserved flags `--source`/`--output` are extracted; every
/// other `--key value` / `--key=value` becomes a template variable. A bare flag
/// (no following value) is treated as `"true"`.
fn parse_build_args(args: &[String]) -> Result<BuildArgs, crate::errors::ValidationError> {
    let mut name: Option<String> = None;
    let mut source: Option<TemplateSource> = None;
    let mut output: Option<String> = None;
    let mut vars: BTreeMap<String, String> = BTreeMap::new();

    let mut i = 0;
    while i < args.len() {
        let token = &args[i];
        if let Some(flag) = token.strip_prefix("--") {
            // Split `--key=value`, or consume the next token as the value unless
            // it is another flag.
            let (key, value) = if let Some((k, v)) = flag.split_once('=') {
                (k.to_string(), v.to_string())
            } else if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                let v = args[i + 1].clone();
                i += 1;
                (flag.to_string(), v)
            } else {
                (flag.to_string(), "true".to_string())
            };

            if key.is_empty() {
                return Err(crate::errors::ValidationError::BuildArgs(format!(
                    "found a bare {} with no flag name",
                    "--".bold()
                )));
            }

            match key.as_str() {
                "source" => {
                    source = Some(parse_source(&value)?);
                }
                "output" => {
                    output = Some(value);
                }
                _ => {
                    vars.insert(key, value);
                }
            }
        } else if name.is_none() {
            name = Some(token.clone());
        } else {
            return Err(crate::errors::ValidationError::BuildArgs(format!(
                "unexpected argument {token:?}"
            )));
        }
        i += 1;
    }

    let name = name.ok_or_else(|| {
        crate::errors::ValidationError::BuildArgs("a template name is required".to_string())
    })?;
    Ok(BuildArgs {
        name,
        source,
        output,
        vars,
    })
}

/// Parse a `--source` value into a `TemplateSource`.
fn parse_source(value: &str) -> Result<TemplateSource, crate::errors::ValidationError> {
    match value.to_ascii_lowercase().as_str() {
        "predefined" => Ok(TemplateSource::Predefined),
        "user" => Ok(TemplateSource::User),
        "project" => Ok(TemplateSource::Project),
        other => Err(crate::errors::ValidationError::BuildArgs(format!(
            "invalid {} {other:?} (expected {}, {}, or {})",
            "--source".bold(),
            "predefined".bold(),
            "user".bold(),
            "project".bold()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_build_args_name_and_vars() {
        let args = svec(&["proxy", "--dataset", "x", "--table", "y"]);
        let parsed = parse_build_args(&args).unwrap();
        assert_eq!(parsed.name, "proxy");
        assert_eq!(parsed.vars.get("dataset").map(String::as_str), Some("x"));
        assert_eq!(parsed.vars.get("table").map(String::as_str), Some("y"));
        assert!(parsed.source.is_none());
        assert!(parsed.output.is_none());
    }

    #[test]
    fn parse_build_args_equals_form() {
        let args = svec(&["proxy", "--dataset=x"]);
        let parsed = parse_build_args(&args).unwrap();
        assert_eq!(parsed.vars.get("dataset").map(String::as_str), Some("x"));
    }

    #[test]
    fn parse_build_args_extracts_reserved_flags() {
        let args = svec(&[
            "proxy",
            "--source",
            "user",
            "--output",
            "models/x.sql",
            "--d",
            "1",
        ]);
        let parsed = parse_build_args(&args).unwrap();
        assert_eq!(parsed.source, Some(TemplateSource::User));
        assert_eq!(parsed.output.as_deref(), Some("models/x.sql"));
        assert_eq!(parsed.vars.get("d").map(String::as_str), Some("1"));
        assert!(!parsed.vars.contains_key("source"));
        assert!(!parsed.vars.contains_key("output"));
    }

    #[test]
    fn parse_build_args_invalid_source_errors() {
        let args = svec(&["proxy", "--source", "bogus"]);
        assert!(parse_build_args(&args).is_err());
    }

    #[test]
    fn parse_build_args_bare_flag_is_true() {
        let args = svec(&["proxy", "--full-refresh"]);
        let parsed = parse_build_args(&args).unwrap();
        assert_eq!(
            parsed.vars.get("full-refresh").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn parse_build_args_missing_name_errors() {
        assert!(parse_build_args(&[]).is_err());
        assert!(parse_build_args(&svec(&["--dataset", "x"])).is_err());
    }

    #[test]
    fn parse_build_args_second_positional_errors() {
        let args = svec(&["proxy", "extra"]);
        assert!(parse_build_args(&args).is_err());
    }

    fn svec(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
}
