use colored::Colorize;
use comfy_table::{Cell, Table, presets::UTF8_FULL};
use dialoguer::Confirm;

use crate::models::alias::{
    ALL_SOURCES, Alias, AliasSource, find_by_name, list_aliases, project_aliases_dir,
    user_aliases_dir, validate_alias_name,
};
use crate::models::config::{ConfigScope, config_dir};
use crate::vprintln;

/// `alias list`: print a table of available aliases. When no source flag is
/// set, all three sources are shown in precedence order (predefined > user >
/// project); otherwise only the flagged sources.
pub fn list(predefined: bool, user: bool, project: bool) {
    let sources: Vec<AliasSource> = if !predefined && !user && !project {
        ALL_SOURCES.to_vec()
    } else {
        ALL_SOURCES
            .into_iter()
            .filter(|s| match s {
                AliasSource::Predefined => predefined,
                AliasSource::User => user,
                AliasSource::Project => project,
            })
            .collect()
    };

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

    let entries = match list_aliases(&sources, &cwd) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("{} Could not list aliases: {e}", "error:".red().bold());
            return;
        }
    };

    if entries.is_empty() {
        println!("{}", "No aliases found.".dimmed());
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL).set_header(vec![
        Cell::new("Source"),
        Cell::new("Name"),
        Cell::new("Definition"),
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
            Cell::new(entry.definition.trim_end()),
            Cell::new(path),
        ]);
    }

    println!("{table}");
}

/// `alias add`: create a new user or project alias.
pub fn add(
    name: String,
    target: AliasSource,
    select: String,
    exclude: Option<String>,
    full_refresh: Option<bool>,
) {
    if let Err(e) = validate_alias_name(&name) {
        eprintln!("{} {e}", "error:".red().bold());
        return;
    }

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

    if target == AliasSource::Project && !crate::utils::is_dbt_project(&cwd) {
        eprintln!(
            "{} a project alias must be created from a dbt project directory (no {} found here).",
            "error:".red().bold(),
            "dbt_project.yml".bold()
        );
        return;
    }

    let entries = match list_aliases(&ALL_SOURCES, &cwd) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("{} Could not inspect existing aliases: {e}", "error:".red().bold());
            return;
        }
    };
    let matches = find_by_name(&entries, &name);

    // Same name already exists in the chosen target: abort.
    if matches.iter().any(|e| e.source == target) {
        eprintln!(
            "{} an alias named {} already exists in {}.",
            "error:".red().bold(),
            name.bold(),
            target.to_string().bold()
        );
        return;
    }

    // Same name exists in a different source: warn and confirm.
    let conflicting: Vec<AliasSource> = matches.iter().map(|e| e.source).collect();
    if !conflicting.is_empty() {
        let where_str = conflicting
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{} an alias named {} already exists in {}. You will need to pass {} every time you reference {} to disambiguate.",
            "warning:".yellow().bold(),
            name.bold(),
            where_str.bold(),
            format!("--source {target}").bold(),
            name.bold(),
        );
        let proceed = Confirm::new()
            .with_prompt("Create it anyway?")
            .default(true)
            .interact()
            .unwrap_or(false);
        if !proceed {
            println!("{}", "Alias not created.".dimmed());
            return;
        }
    }

    // Resolve the destination directory.
    let dir = match target {
        AliasSource::User => {
            let (config_root, _) = match config_dir(Some(ConfigScope::Global)) {
                Ok(resolved) => resolved,
                Err(e) => {
                    eprintln!("{} Could not resolve global config directory: {e}", "error:".red().bold());
                    return;
                }
            };
            if !config_root.exists() {
                eprintln!(
                    "{} the global config directory does not exist yet. Run {} first.",
                    "error:".red().bold(),
                    "dbt-assist setup --scope global".bold()
                );
                return;
            }
            match user_aliases_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    eprintln!("{} Could not resolve user aliases directory: {e}", "error:".red().bold());
                    return;
                }
            }
        }
        AliasSource::Project => project_aliases_dir(&cwd),
        AliasSource::Predefined => {
            eprintln!(
                "{} predefined aliases are bundled and cannot be created.",
                "error:".red().bold()
            );
            return;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("{} Could not create {}: {e}", "error:".red().bold(), dir.display());
        return;
    }

    let alias = Alias {
        select,
        exclude: exclude.filter(|s| !s.is_empty()),
        full_refresh,
    };
    let yaml = match serde_yml::to_string(&alias) {
        Ok(yaml) => yaml,
        Err(e) => {
            eprintln!("{} Could not serialize alias: {e}", "error:".red().bold());
            return;
        }
    };
    let path = dir.join(format!("{name}.yml"));
    if let Err(e) = std::fs::write(&path, yaml) {
        eprintln!("{} Could not write {}: {e}", "error:".red().bold(), path.display());
        return;
    }
    vprintln!("Wrote {}", path.display());

    println!(
        "{} alias {} created in {}.",
        "✓".green().bold(),
        name.bold(),
        target.to_string().bold()
    );
    println!(
        "  Run it with {}.",
        format!("dbt-assist jobs run {name} --source {target}").bold()
    );
}

/// `alias remove`: delete a user or project alias by name. Predefined aliases
/// are immutable.
pub fn remove(name: String, source: Option<AliasSource>) {
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

    let entries = match list_aliases(&ALL_SOURCES, &cwd) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("{} Could not list aliases: {e}", "error:".red().bold());
            return;
        }
    };
    let matches = find_by_name(&entries, &name);

    if matches.is_empty() {
        eprintln!("{} no alias named {} found.", "error:".red().bold(), name.bold());
        return;
    }

    // Collect the entries we're allowed to delete (predefined aliases are
    // bundled and immutable), narrowed to `--source` when given.
    let targets: Vec<&_> = match source {
        Some(src) => {
            let found: Vec<&_> = matches.iter().filter(|e| e.source == src).copied().collect();
            if found.is_empty() {
                eprintln!(
                    "{} no alias named {} found in {}.",
                    "error:".red().bold(),
                    name.bold(),
                    src.to_string().bold()
                );
                return;
            }
            found
        }
        None => matches
            .iter()
            .filter(|e| e.source != AliasSource::Predefined)
            .copied()
            .collect(),
    };

    if targets.is_empty() {
        // The only matches were predefined.
        eprintln!(
            "{} {} is a predefined alias and cannot be removed.",
            "error:".red().bold(),
            name.bold()
        );
        return;
    }

    // Only confirm when more than one file would be deleted; a single match is
    // removed outright.
    if targets.len() > 1 {
        let where_str = targets
            .iter()
            .map(|e| e.source.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let proceed = Confirm::new()
            .with_prompt(format!(
                "Found {} aliases named {name} ({where_str}). Remove all of them?",
                targets.len()
            ))
            .default(false)
            .interact()
            .unwrap_or(false);
        if !proceed {
            println!("{}", "Aliases not removed.".dimmed());
            return;
        }
    }

    for target in &targets {
        let Some(path) = &target.path else {
            eprintln!("{} alias {} has no on-disk path.", "error:".red().bold(), name.bold());
            continue;
        };
        match std::fs::remove_file(path) {
            Ok(()) => println!(
                "{} alias {} removed from {}.",
                "✓".green().bold(),
                name.bold(),
                target.source.to_string().bold()
            ),
            Err(e) => {
                eprintln!("{} Could not remove {}: {e}", "error:".red().bold(), path.display())
            }
        }
    }
}
