# Configuration

All configuration for dbt-assist lives in a single `config.yaml`. It has four top-level fields:

```yaml
dbt_api_connection: { ... }   # how to talk to dbt (required, tagged by `type`)
manifest_storage:   { ... }   # where production manifest.json lives (required, tagged by `type`)
service_account_path: ...     # optional: path to a GCP service-account JSON key
project: ...                  # optional: GCP project id (falls back to the SA's project_id)
```

## Scopes

`config.yaml` can live in one of two scopes, selectable on most commands via `--scope local|global`:

- **Local** — `./.dbt-assist/config.yaml`, kept inside a single dbt project.
- **Global** — a per-user config folder for your operating system. On Linux this is `~/.config/dbt-assist/` (more precisely `$XDG_CONFIG_HOME/dbt-assist/`).

When you **omit** `--scope`, the tool *finds* the config in this order:

1. The `DBT_ASSIST_CONFIG_DIR` environment variable — if it's set **and** the folder exists (treated as Global).
2. `./.dbt-assist/` in the current directory — if present (treated as Local).
3. The global folder above.

When you **pass** `--scope`, it goes straight to that scope's folder — it does not check the environment variable or whether the folder exists.

## dbt API connection types

`dbt_api_connection` has a `type:` line that selects one of three ways to reach dbt. Most fields below are collected for you interactively by `setup`.

**`direct`** — talks to the dbt Cloud Admin API v2 directly with a `Bearer` token. Because it isn't fronted by a proxy, it carries the most config:

```yaml
dbt_api_connection:
  type: direct
  dbt_api_url: https://cloud.getdbt.com/api
  dbt_api_token: <your-dbt-cloud-token>
  account_id: 12345
  dbt_assist_job_name: dbt-assist          # the job dedicated to dbt-assist runs
  dbt_target_name: prod                     # default: prod
  username: alice                           # optional; default "user" (used in run cause)
```

**`gcp_function_proxy`** — talks to a Google Cloud Function fronting dbt. Auth is either a static API key or a freshly minted GCP ID token (audience = the function URL), depending on `auth_with_service_account`:

```yaml
dbt_api_connection:
  type: gcp_function_proxy
  endpoint_url: https://region-project.cloudfunctions.net/dbt-proxy
  auth_with_service_account: true           # true → mint GCP ID token; false → API key
```

**`normal_proxy`** — talks to any HTTP proxy implementing the dbt-assist proxy API, with optional HTTP Basic auth:

```yaml
dbt_api_connection:
  type: normal_proxy
  proxy_url: https://dbt-proxy.example.com
  proxy_username: alice                      # optional
  proxy_password: <secret>                   # optional
```

See [dbt API connection & custom proxy](README.md#dbt-api-connection--custom-proxy) for what the two proxy transports expect on the wire.

## Manifest storage types

`manifest_storage` also has a `type:` line, telling the `manifest` command where the production `manifest.json` comes from:

```yaml
# Local filesystem (tilde expansion supported):
manifest_storage:
  type: local
  path: ~/shared/dbt-prod

# Google Cloud Storage:
manifest_storage:
  type: gcs
  bucket: my-dbt-artifacts
  path: manifests                            # prefix inside the bucket
  test_file: manifests/healthcheck.txt       # an object used to validate bucket access in `setup`
```

For GCS the object key is built as `<path>/<project>/manifest.json`, where `<project>` is the dbt project name.

## "Hidden" config options

Some `direct` fields are **not** prompted by `setup` — add them to `config.yaml` by hand if you need them:

- `default_threads_num` (default `1`) — thread count for a normal run.
- `turbo_threads_num` (default `4`) — thread count when `--turbo` is passed.

```yaml
dbt_api_connection:
  type: direct
  # ...the prompted fields above...
  default_threads_num: 2
  turbo_threads_num: 8
```

Also note: `project` (top-level) is optional — if unset, GCP operations fall back to the service account's own `project_id`.

## Validating config

`setup` validates the config after writing it — it pings the dbt connection, checks the service account, and checks GCS access. To validate an **existing** config without writing anything, use `setup --test-only`.
