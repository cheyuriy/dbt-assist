# Proxy API specification

This document specifies the HTTP API that a **dbt-assist proxy** must implement
so the `dbt-assist` CLI can talk to it. It is intentionally language-agnostic:
it describes the wire contract (endpoints, request/response shapes, status codes,
errors, and authorization) without reference to any particular implementation
language or framework.

`dbt-assist` supports two proxy transports — a **normal HTTP proxy** and a
**GCP Cloud Function proxy**. Both expose the **identical** API described here;
they differ **only** in how requests are authorized (see
[Authorization](#authorization)). A single server implementation can therefore
serve either role; the deployment context decides which authorization scheme is
enforced.

> The third transport, **Direct**, bypasses the proxy and talks to the dbt Cloud
> Admin API v2 itself; it is **not** covered here. See [api.md](api.md).

## What the proxy does

The proxy is a thin façade in front of the **dbt Cloud Admin API v2**. For each
endpoint below it accepts a request from the CLI, performs one or more dbt Cloud
API calls (resolving the project, locating the dbt-assist job, triggering /
inspecting / cancelling runs), and returns a response in the shape the CLI
expects. The dbt Cloud operations involved are the same ones the Direct
connection uses:

- List Projects, List Jobs, Retrieve Job, Update Job
- Trigger Job Run, Retrieve Run, List Runs, Cancel Run

Full dbt Cloud documentation: <https://docs.getdbt.com/dbt-cloud/api-v2>.

How the proxy maps these endpoints onto dbt Cloud (resolving the project by
name, selecting the dedicated dbt-assist job, formatting the `dbt build` step,
remapping status codes, etc.) is an implementation detail of the proxy and out
of scope for this contract — only the request/response shapes below are binding.

## Conventions

- **Base URL.** The CLI is configured with the proxy's base URL. It may be
  given with or without a trailing slash; the client normalizes it (a trailing
  slash is trimmed) before appending the endpoint path. Endpoints below are
  shown relative to the base URL.
- **Content type.** All request and response bodies are JSON. Send
  `Content-Type: application/json` on responses with a body.
- **Encoding.** UTF-8.
- **At most one auth header.** The client sends at most one `Authorization`
  header per request (see below).

## Authorization

All authorization is carried in the standard `Authorization` request header.
The client sends one of three schemes depending on how the connection is
configured. The same `/...` endpoints are used regardless of scheme — only the
header differs. A proxy should enforce whichever scheme matches its deployment
and reject unauthorized requests (typically on `GET /ping` first).

| Scheme | Header sent | Used by | When |
| --- | --- | --- | --- |
| **None** | *(no `Authorization` header)* | either proxy | No credentials configured. |
| **Basic** | `Authorization: Basic <base64(username:password)>` | normal HTTP proxy | A username **and** password are both configured. |
| **Bearer** | `Authorization: Bearer <google-id-token>` | GCP Cloud Function proxy | Service-account authentication is enabled. |

Notes:

- **None.** Either proxy type sends no `Authorization` header when no
  credentials are configured. Use this only for proxies that are otherwise
  protected (e.g. network isolation).
- **Basic** (normal proxy). Standard HTTP Basic authentication: the base64 of
  `username:password`. The client sends it **only** when both a username and a
  password are configured; if either is missing it sends no header.
- **Bearer** (GCP Cloud Function proxy). The client mints a **Google-signed ID
  token** whose **audience (`aud`) is the proxy's function URL** (the same base
  URL the client calls), and sends it as a Bearer token. This is the standard
  mechanism for authenticating to a Google Cloud Function / Cloud Run service:
  the platform (or your handler) validates the token's signature and audience.
  The client sends it **only** when service-account authentication is enabled;
  otherwise it sends no header.

## Endpoints

Summary:

| Method & path | Purpose | Request body | Success |
| --- | --- | --- | --- |
| `GET /ping` | Health + auth check | — | `200` |
| `GET /jobs/manual/queue` | Active/queued runs for the project's manual job | `{ project_name }` | `200` |
| `POST /runs/manual` | Create (trigger) a run | run spec (below) | `201` |
| `GET /runs/:id` | Status of a single run | — | `200` |
| `DELETE /runs/:id` | Cancel a run | — | `200` |

---

### `GET /ping`

Liveness and authorization check. The client calls this to verify the proxy is
reachable and that its credentials (if any) are accepted.

- **Request body:** none.
- **Success:** `200 OK` (body ignored).
- **Failure:** any non-`200` status is treated as failure (e.g. `401`/`403` for
  bad or missing credentials, `5xx` if the proxy can't reach dbt Cloud).

---

### `GET /jobs/manual/queue`

Returns the queue of currently active or queued runs for the dbt-assist manual
job of the given project.

> **Note:** this is a `GET` request that carries a **JSON request body**. A
> conforming proxy must read the project name from the request body, not from a
> query string.

- **Request body:**

  | Field | Type | Required | Notes |
  | --- | --- | --- | --- |
  | `project_name` | string | yes | dbt project name to resolve. |

  ```json
  { "project_name": "analytics" }
  ```

- **Success:** `200 OK` with:

  | Field | Type | Required | Notes |
  | --- | --- | --- | --- |
  | `active_runs` | integer | yes | Number of runs in the queue. Part of the contract; the current client reads only `runs`. |
  | `runs` | array | yes | One entry per active/queued run (may be empty). |
  | `runs[].id` | integer | yes | Run id. |
  | `runs[].status` | integer | yes | Normalized status code — see below. |
  | `runs[].run_duration_humanized` | string | yes | Human-readable elapsed run time. |
  | `runs[].queued_duration_humanized` | string | yes | Human-readable time spent queued. |
  | `runs[].trigger` | object | yes | `{ "cause": string }` — what triggered the run (typically the user). |
  | `runs[].job` | object | yes | `{ "execute_steps": [string] }` — the dbt commands the run executes. |

  **Status codes (`runs[].status`).** The client maps these to labels and
  expects the proxy to emit already-normalized codes:

  | Code | Label |
  | --- | --- |
  | `0` | Queued |
  | `1` | Starting |
  | `2` | Running |
  | other | Unknown |

  ```json
  {
    "active_runs": 1,
    "runs": [
      {
        "id": 1234,
        "status": 2,
        "run_duration_humanized": "1h12m",
        "queued_duration_humanized": "0s",
        "trigger": { "cause": "user@example.com via dbt-assist" },
        "job": { "execute_steps": ["dbt build --select tag:nightly"] }
      }
    ]
  }
  ```

- **Failure:** `400` (malformed body / missing `project_name`), `500`
  (unexpected error or error talking to dbt Cloud).

---

### `POST /runs/manual`

Creates (triggers) a new run of the project's dbt-assist manual job, building
the selected models.

- **Request body:**

  | Field | Type | Required | Notes |
  | --- | --- | --- | --- |
  | `select` | string | yes | dbt selector for the models to build. |
  | `project_name` | string | yes | dbt project name to resolve. |
  | `exclude` | string | no | dbt exclude selector. **Omitted entirely** from the body when not set. |
  | `full_refresh` | boolean \| null | no | Tri-state: `true`, `false`, or `null`. **`null` is distinct from `false`** — `null`/absent means "leave to the model's own config", `false` means "force no full refresh". Always present in the body (as `null` when unset). |
  | `turbo` | boolean | no | Whether to run with the higher "turbo" thread count. |

  ```json
  {
    "select": "tag:nightly",
    "project_name": "analytics",
    "exclude": "model_x",
    "full_refresh": true,
    "turbo": false
  }
  ```

  Minimal example (no `exclude`, `full_refresh` unset):

  ```json
  {
    "select": "*",
    "project_name": "analytics",
    "full_refresh": null,
    "turbo": true
  }
  ```

- **Success:** `201 Created` with:

  | Field | Type | Required | Notes |
  | --- | --- | --- | --- |
  | `run_id` | integer | yes | Id of the newly created run. |

  ```json
  { "run_id": 1234 }
  ```

- **Failure:** `400` (malformed body / missing required fields), `500`
  (unexpected error or error talking to dbt Cloud).

---

### `GET /runs/:id`

Returns the detailed status of a single run, including per-step status and logs.

- **Path parameters:**

  | Param | Type | Notes |
  | --- | --- | --- |
  | `id` | integer | Run id. The endpoint is keyed only by id; no project name is sent. |

- **Request body:** none.
- **Success:** `200 OK` with:

  | Field | Type | Required | Notes |
  | --- | --- | --- | --- |
  | `in_progress` | boolean | yes | Run is currently executing. |
  | `is_complete` | boolean | yes | Run has finished (success, error, or cancelled). |
  | `is_success` | boolean | yes | Run finished successfully. |
  | `is_error` | boolean | yes | Run finished with an error. |
  | `is_cancelled` | boolean | yes | Run was cancelled. |
  | `duration` | string | yes | Human-readable total duration. |
  | `status_humanized` | string | yes | Human-readable status string. |
  | `run_steps` | array | yes | One entry per step (may be empty before steps exist). |
  | `run_steps[].name` | string | yes | Step name. |
  | `run_steps[].index` | integer | yes | 1-based step index. |
  | `run_steps[].status_humanized` | string | yes | Human-readable step status. |
  | `run_steps[].logs` | string \| null | yes (nullable) | Step logs; `null` until the step has produced them. |
  | `run_steps[].debug_logs` | string \| null | yes (nullable) | Step debug logs; `null` until available. |

  The boolean flags are mutually informative: a run is either in progress, or
  complete (and then success / error), or cancelled. The client derives its
  label in this order: cancelled → in-progress (or not-yet-complete) → success →
  failed.

  ```json
  {
    "in_progress": false,
    "is_complete": true,
    "is_success": true,
    "is_error": false,
    "is_cancelled": false,
    "duration": "00:01:00",
    "status_humanized": "Success",
    "run_steps": [
      {
        "name": "Invoke dbt with `dbt build`",
        "index": 1,
        "status_humanized": "Success",
        "logs": "...",
        "debug_logs": null
      }
    ]
  }
  ```

- **Failure:** `500` (run not found, no permission to access it, or any other
  error talking to dbt Cloud).

---

### `DELETE /runs/:id`

Cancels a run.

- **Path parameters:**

  | Param | Type | Notes |
  | --- | --- | --- |
  | `id` | integer | Run id. Keyed only by id; no project name is sent. |

- **Request body:** none.
- **Success:** `200 OK` (body ignored).
- **Failure:** `404` (run not found or already cancelled), `500` (unexpected
  error or error talking to dbt Cloud).

## Error format

Every non-success response **SHOULD** carry a JSON body with a `message` field
describing the reason:

```json
{ "message": "project 'analytics' not found" }
```

The client surfaces `message` to the user. If the body is missing or not
parseable as this shape, the client falls back to reporting just the HTTP status
code. Returning a `message` is therefore strongly recommended for every error
response.

## Status-code summary

| Endpoint | Success | Error codes |
| --- | --- | --- |
| `GET /ping` | `200` | any non-`200` |
| `GET /jobs/manual/queue` | `200` | `400`, `500` |
| `POST /runs/manual` | `201` | `400`, `500` |
| `GET /runs/:id` | `200` | `500` |
| `DELETE /runs/:id` | `200` | `404`, `500` |

Authorization failures (when a scheme is enforced) typically surface as `401` /
`403`; the client treats any non-success status as an error and shows the
`message`.

## Notes for implementers

- **Extra fields are ignored; missing required fields fail the client.** The
  client deserializes only the fields documented above. You may include
  additional fields in responses, but every field marked **required** must be
  present (nullable fields may be `null`).
- **`GET /jobs/manual/queue` takes a JSON body.** Despite being a `GET`, the
  project name is in the request body, not the query string.
- **Preserve the `full_refresh` tri-state.** Treat `null`/absent differently
  from `false` when triggering the run.
- **Emit normalized queue status codes** (`0`/`1`/`2`) from
  `GET /jobs/manual/queue`; the client does not re-map them.
- **Per-step `duration`.** Earlier informal notes ([api.md](api.md)) list a
  `duration` field on each `run_steps[]` entry of `GET /runs/:id`. The current
  client does **not** read it, so it is optional/ignored — included here only to
  reconcile the two documents.
