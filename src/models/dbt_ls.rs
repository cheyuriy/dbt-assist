//! Parsing and counting of `dbt ls --output json` output, used by the
//! `jobs manual` pre-flight build-impact check. Kept free of I/O so it can be
//! unit-tested; the subprocess invocation lives in `commands::jobs`.

use serde::Deserialize;

#[derive(Deserialize)]
struct LsRecord {
    #[allow(dead_code)]
    unique_id: String,
    config: LsConfig,
}

#[derive(Deserialize)]
struct LsConfig {
    #[serde(default)]
    materialized: Option<String>,
    #[serde(default)]
    full_refresh: Option<bool>,
}

/// Counts of the models a selection would build, restricted to materializations
/// that actually persist data (`table` and `incremental`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BuildImpact {
    /// Total affected models (`tables` + `incrementals`).
    pub total: usize,
    pub tables: usize,
    pub incrementals: usize,
    /// Incremental models that would be fully refreshed when the command runs
    /// with `--full-refresh true`: those whose `config.full_refresh` is `true`
    /// or unset (`null`). Models pinning `full_refresh: false` opt out.
    pub full_refresh: usize,
}

/// Parse the JSON-lines stdout of `dbt ls --output json`. Lines that don't parse
/// as a record (stray non-JSON output) are skipped, as are records whose
/// materialization is neither `table` nor `incremental`.
pub fn parse_build_impact(stdout: &str) -> BuildImpact {
    let mut impact = BuildImpact::default();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<LsRecord>(line) else {
            continue;
        };
        match record.config.materialized.as_deref() {
            Some("table") => {
                impact.total += 1;
                impact.tables += 1;
            }
            Some("incremental") => {
                impact.total += 1;
                impact.incrementals += 1;
                // Absent (`null`) full_refresh defaults to being refreshed.
                if record.config.full_refresh.unwrap_or(true) {
                    impact.full_refresh += 1;
                }
            }
            _ => {}
        }
    }

    impact
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_tables_and_incrementals_excludes_others() {
        let stdout = "\
{\"unique_id\": \"model.p.a\", \"config\": {\"materialized\": \"table\"}}
{\"unique_id\": \"model.p.b\", \"config\": {\"materialized\": \"incremental\", \"full_refresh\": null}}
{\"unique_id\": \"model.p.c\", \"config\": {\"materialized\": \"view\"}}
{\"unique_id\": \"model.p.d\", \"config\": {\"materialized\": \"ephemeral\"}}";
        let impact = parse_build_impact(stdout);
        assert_eq!(impact.total, 2);
        assert_eq!(impact.tables, 1);
        assert_eq!(impact.incrementals, 1);
    }

    #[test]
    fn full_refresh_counts_true_and_null_not_false() {
        let stdout = "\
{\"unique_id\": \"model.p.a\", \"config\": {\"materialized\": \"incremental\", \"full_refresh\": true}}
{\"unique_id\": \"model.p.b\", \"config\": {\"materialized\": \"incremental\", \"full_refresh\": null}}
{\"unique_id\": \"model.p.c\", \"config\": {\"materialized\": \"incremental\", \"full_refresh\": false}}";
        let impact = parse_build_impact(stdout);
        assert_eq!(impact.incrementals, 3);
        assert_eq!(impact.full_refresh, 2);
    }

    #[test]
    fn skips_malformed_lines() {
        let stdout = "\
{\"unique_id\": \"model.p.a\", \"config\": {\"materialized\": \"table\"}}
this is not json

{\"unique_id\": \"model.p.b\", \"config\": {\"materialized\": \"table\"}}";
        let impact = parse_build_impact(stdout);
        assert_eq!(impact.total, 2);
        assert_eq!(impact.tables, 2);
    }

    #[test]
    fn empty_input_is_all_zero() {
        assert_eq!(parse_build_impact(""), BuildImpact::default());
    }
}
