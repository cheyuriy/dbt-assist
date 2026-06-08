# dbt-assist

A command-line tool to support your work with dbt from a local environment. It helps you **run, manage, and monitor dbt jobs in production**, plus a few quality-of-life features (model templates, run aliases) that make day-to-day dbt work easier.

---

## Quick start

### Install

Install straight from the repository with Cargo:

```bash
cargo install --git https://github.com/cheyuriy/dbt-assist dbt-assist
```

To pin a specific release, add a tag:

```bash
cargo install --git https://github.com/cheyuriy/dbt-assist --tag v0.1.0 dbt-assist
```

Or build it manually from a clone:

```bash
git clone https://github.com/cheyuriy/dbt-assist
cd dbt-assist
cargo build --release        # binary ends up in target/release/dbt-assist
```

Either way you get a `dbt-assist` executable. `--verbose` is a global flag valid on any subcommand; it toggles extra diagnostic output.

### A minimal happy path

```bash
dbt-assist setup                 # interactively build & validate config.yaml
# ...then, from inside a dbt project root:
dbt-assist init                  # scaffold the project for dbt-assist
dbt-assist manifest              # pull production manifest.json for `defer`
dbt-assist jobs run daily --watch  # run a saved alias and watch it complete
```

---

## Configuration

All settings live in a single `config.yaml` — its scopes, the three dbt API connection types, manifest storage, and the extra "hidden" options are documented in **[CONFIG.md](CONFIG.md)**. The preferred way to create it is `setup`, which builds and validates the file for you through an interactive wizard; editing `config.yaml` by hand is only needed for the "hidden" options.

---

## `init` command

`init` scaffolds the current dbt project for dbt-assist and the **"Power User for dbt"** VSCode extension. It must be run from a dbt project root (it bails if there's no `dbt_project.yml`).

It does the following:

1. Creates the hidden working dirs the CLI uses inside the project:
   - `.manifest` — the production `manifest.json` that `manifest` downloads (used by dbt's `defer`).
   - `.aliases` — project-level run [aliases](#aliases).
   - `.templates` — project-level model [templates](#templates).
   - `.logs` — run logs saved with `--save-files`.
   - `.dbt-assist` — the local-scope `config.yaml`.
2. **(optional, default yes)** Configures `.vscode/settings.json`, **preserving** your existing settings. It points dbt's defer config at `.manifest/manifest.json`, enables the new lineage panel, and adds `*.sql`/`*.yml` file associations. Nested sub-projects are detected and you're warned to configure them manually.
3. **(optional, default yes)** Updates `.gitignore` — appends the created folders plus `.vscode`, skipping entries that are already present (creating the file if absent).

Both confirmations are asked **before** the final success message.

---

## Commands reference

This is the high-level map of every command. The template, alias, `jobs manual`, and proxy details have their own sections below. Every command and flag is also documented in the built-in CLI help — append `--help` to any command (e.g. `dbt-assist jobs manual --help`).

### Common flags

Several flags recur across commands; they mean the same thing everywhere:

- `--scope local|global` — which config scope to load (see [Scopes](CONFIG.md#scopes)).
- `--project-name <name>` — override the dbt-cloud project name (otherwise taken from `name:` in `dbt_project.yml`).
- `--watch` — poll the run to completion, redrawing a live status table.
- `--logs-always` — print step logs at the end even when the run succeeded (default: only on failure).
- `--debug-logs` — print/save debug logs instead of normal logs.
- `--save-files` — write each step's logs to `.logs/<run_id>/` (requires a dbt project root).
- `--yes` / `--force` — auto-confirm prompts (the pre-flight checks still run; see [`jobs manual`](#jobs-manual-deep-dive)).

### Commands

- **`setup [--scope] [--test-only]`** — interactive wizard to build, save, and validate `config.yaml`. `--test-only` validates an existing config without writing.
- **`init`** — scaffold the current dbt project (see above).
- **`manifest [--scope] [--project-name] [--manifest-dir <dir>]`** — refreshes the local `manifest.json` (so dbt's `defer` has up-to-date production state) by copying it from the configured `manifest_storage` into `--manifest-dir` (default `.manifest`). It reports the **source's** last-modified time and warns when the manifest is ≥ 24 hours old. Must run from a dbt project root.
- **`runs queue [--scope] [--project-name]`** — lists active/queued runs of the production job as a table (Run ID / Status / In run / In queue / Run by / Task), or "No active or queued runs."
- **`runs cancel <run_id> [--scope] [--project-name]`** — cancels a run by id.
- **`runs check <run_id> [--scope] [--project-name] [--logs-always] [--debug-logs] [--save-files]`** — prints a one-row status table (Run status / Duration / Steps / Logs / Debug logs / Logs directory) with one glyph per step. Step logs are printed after the table when the run failed or `--logs-always` is set; `--debug-logs` swaps in the debug logs; `--save-files` writes them to `.logs/<run_id>/`. `jobs manual` reuses this command's machinery and flags.
- **`jobs run <alias> [--source] [--turbo] [--scope] [--watch] [--logs-always] [--debug-logs] [--save-files] [--yes/--force]`** — runs a saved [alias](#aliases) (see that section).
- **`jobs manual <select> ...`** — runs an ad-hoc selection of models. Has the most logic — see [its own section](#jobs-manual-deep-dive).
- **`alias list/add/remove`** — manage run [aliases](#aliases).
- **`templates list/docs/build`** — manage model [templates](#templates).

---

## `jobs manual` deep dive

```
jobs manual <select> [--exclude <q>] [--full-refresh <bool>] [--turbo] \
            [--project-name] [--scope] [--watch] [--logs-always] [--debug-logs] \
            [--save-files] [--yes/--force]
```

`<select>` is the required dbt selector for the models to build. `--full-refresh` is tri-state: **absent is not the same as `false`** (absent leaves it to each model's config). `jobs manual` must run from a dbt project root.

It works in three steps.

### Step 1 — Pre-flight checks

Both checks run even when `--yes`/`--force` is set (that flag only auto-confirms; the checks still execute). All prompts default to **no**.

- **Build impact** — runs `dbt ls` in your current shell to count the models your selection would build, and prints the stats. If the count exceeds **40** models, it asks you to confirm before continuing. If `dbt` isn't found or fails, it asks whether to continue without the check.
- **Queue** — reuses the dbt connection to list the run queue; if anything is active or queued it warns ("your run may be delayed") and asks to confirm.

### Step 2 — Trigger

Creates the run with your `select` / `exclude` / `full_refresh`, using `turbo_threads_num` if `--turbo` is set (else `default_threads_num`), and returns the new run id.

### Step 3 — Watch (only with `--watch`)

Polls the run and redraws the status table **in place** as it progresses (it updates the existing table rather than clearing the screen, so old frames don't pile up in your scrollback). Once the run reaches a final state it polls a couple more times so the logs can finish loading, then saves (`--save-files`) and/or prints logs (`--logs-always`, or on failure; `--debug-logs` to show debug logs instead). Watching stops on its own after at most ~5 minutes.

Without `--watch`, it just prints the new run id and a `runs check` hint.

---

## Templates

Templates are **Jinja2 files that render *into* a dbt model**. Because the rendered output is itself dbt Jinja, templates lean heavily on `{% raw %}` to protect dbt's own `{{ ... }}` from being evaluated at template time.

Recognized extensions: `.jinja`, `.j2`, `.jinja2` (the filename without the extension is the template name).

### Sources

Templates come from three sources, in precedence order **predefined > user > project**:

- **Predefined** — shipped with the CLI. Immutable.
- **User** — the `templates/` folder inside your global config dir.
- **Project** — the `.templates/` folder in your current dbt project.

### Custom tags

On top of standard Jinja, a template may carry two custom tags. These are preprocessed before rendering and **must live outside any `{% raw %}` block**:

- `{% output '<path>' %}` — the quoted argument is itself a Jinja template; interpolated with your build vars it yields the output file path. Only one `{% output %}` is allowed.
- `{% docs %} … {% enddocs %}` — verbatim documentation, shown by `templates docs`.

### Rendering

Rendering is **strict** about variables: if the template uses a variable you didn't provide, it's an error — so you must pass every variable it references (extra variables you pass are simply ignored). Content inside `{% raw %}` is never evaluated, and nothing is HTML-escaped.

### Subcommands

- **`templates list [--predefined] [--user] [--project]`** — table of Source / Name / Path (no flag = all three sources). Runs anywhere.
- **`templates docs <name> [--source predefined|user|project]`** — prints the `{% docs %}` content (or `(no docs)`) and the **raw, pre-interpolation** `{% output %}` expression. Runs anywhere.
- **`templates build <name> [--source …] [--output <path>] [--key value …]`** — renders the body and writes the model. Must run from a dbt project root. Everything after `build` is parsed leniently: the first non-flag token is the template name; `--source` and `--output` are reserved (`--output` is used **literally**, not interpolated); every other `--key value` or `--key=value` becomes a template variable, and a bare flag (no following value) becomes `"true"`. A second positional argument is an error. The output path is `--output` if given, otherwise the interpolated `{% output %}` tag. It confirms before overwriting an existing file (default no).

### Worked example

A trimmed version of the bundled [proxy.jinja](templates/proxy.jinja):

```jinja
{% if prev_day is defined and prev_day %}{% set suffix = "_prev_day" %}
{% else %}{% set suffix = "" %}{% endif %}
{% output 'models/proxies/{{dataset}}/{{dataset}}_{{table}}{{suffix}}.sql' %}
{% docs %}
Creates a proxy model for a provided table.
Mandatory: `--dataset`, `--table`.
{% enddocs %}
{% raw %}
{{ config(materialized='ephemeral') }}
SELECT * FROM {{ source('{% endraw %}{{ dataset }}{% raw %}', '{% endraw %}{{ table }}{% raw %}') }}
{% endraw %}
```

Build it:

```bash
dbt-assist templates build proxy --dataset=analytics --table=users --prev-day
```

- The bare `--prev-day` flag becomes the variable `prev-day = "true"`, so `suffix` becomes `_prev_day`.
- `{{ dataset }}` → `analytics`, `{{ table }}` → `users` (outside the `{% raw %}` block).
- The `{% output %}` tag interpolates to `models/proxies/analytics/analytics_users_prev_day.sql`, where the file is written.
- The `{% raw %}` block is preserved verbatim, so dbt's `{{ config(...) }}` / `{{ source(...) }}` end up in the rendered model untouched.

---

## Aliases

An alias is a small YAML file naming a reusable model selection. The filename without its extension is the alias name. Its fields:

```yaml
select: tag:daily       # required; the dbt selector (default "*")
exclude: tag:slow       # optional
full_refresh: true      # optional, tri-state — an absent value is NOT the same as false
```

Aliases use the same three sources and precedence as templates (**predefined > user > project**): predefined ones ship with the CLI, user ones live in the `aliases/` folder of your global config dir, and project ones in `.aliases/` in your current dbt project. Names are matched case-insensitively and may not be empty or contain `/`, `\`, or `.`.

The CLI ships a predefined set: `all` (`select: "*"`), `daily`, `hourly`, `weekly`, `monthly` (e.g. [daily.yml](aliases/daily.yml) is just `select: tag:daily`).

### Subcommands

- **`alias list [--predefined] [--user] [--project]`** — table of all aliases (no flag = all three sources).
- **`alias add <name> [--target user|project] [--select] [--exclude] [--full-refresh]`** — `--target` defaults to `project` (predefined aliases are immutable, so they can't be a target). A project target requires a dbt project root; a user target requires the global config dir to already exist (run `setup --scope global` first). It aborts if the name already exists in the chosen target, and warns + confirms if it exists in a *different* source.
- **`alias remove <name> [--source user|project]`** — predefined aliases can't be removed. Without `--source` it removes all non-predefined matches (confirming when more than one file would be deleted); `--source` narrows it.

### Running an alias

```bash
dbt-assist jobs run daily --watch
```

`jobs run <alias>` resolves the alias across all sources (pass `--source` to disambiguate when the name exists in more than one) and passes its `select` / `exclude` / `full_refresh` straight into `jobs manual` — so the entire [`jobs manual`](#jobs-manual-deep-dive) flow (pre-flight checks, `--turbo`, `--watch`, logs/file behavior) applies identically.

---

## dbt API connection & custom proxy

All three transports implement the same `DbtApiClient` trait (`ping`, `get_runs_queue`, `create_run`, `check_run_status`, `cancel_run`); the connection `type` in your config selects one:

- **`direct`** orchestrates the dbt Cloud Admin API v2 itself (resolving the project and the dbt-assist job, updating the job's build step + thread count, triggering and polling runs).
- **`normal_proxy`** and **`gcp_function_proxy`** speak a small **custom proxy API** and differ only in how they authenticate. The proxy must implement: `GET /ping`, `GET /jobs/manual/queue`, `POST /runs/manual`, and `GET` / `DELETE /runs/:id`.

If you want to stand up your own proxy, the full contract — endpoints, request/response shapes, auth schemes, error format, the tri-state `full_refresh`, and the pre-normalized status codes — is documented in **[PROXY_API.md](PROXY_API.md)**.

---

## Testing

```bash
cargo test                      # run all tests
cargo test <name_substring>     # run a single test by name
cargo test config::             # run tests in one module
cargo clippy --all-targets      # lint
```

### Tests that need real GCP credentials

Some tests in `src/gcp/client.rs` and `src/api/gcp_function_proxy.rs` mint real Google-signed ID tokens or download from GCS, so they need a service account and network access. They read these from a **gitignored `.env.test`** (or the environment) — see [.env.test.example](.env.test.example):

```dotenv
TEST_SERVICE_ACCOUNT_PATH="path-to-service-account.json"  # a service-account JSON key
TEST_GCS_BUCKET="your-gcs-bucket"                          # an existing bucket...
TEST_GCS_OBJECT="your-gcs-object-with-path"                # ...and object for download tests
```

Without these vars, those specific tests panic. Everything else — path resolution, error handling, `auth_with_service_account = false`, and the `httpmock`-based connector tests — runs offline.

Tests that touch process-global state (env vars, cwd) are marked `#[serial]`; keep that attribute when adding similar tests, since `env::set_var` is `unsafe` in Rust 2024 precisely because it isn't thread-safe.

---

## Contributing

For architecture and code-level notes, see [CLAUDE.md](CLAUDE.md).
