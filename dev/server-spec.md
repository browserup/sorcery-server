# Sorcery Server - Implementation Specification

**Version:** 1.1
**Audience:** Server implementers (any language)
**Purpose:** Define HTTP server behavior for bridging web repo URLs to the `srcuri://` protocol

---

## Overview

Sorcery Server provides two complementary modes:

1. **Workspace Mirror** - HTTPS mirror of `srcuri://` protocol links for platforms that don't support custom protocols (Slack, Teams, Jira)

2. **Provider Passthrough** - Converts web repository URLs (GitHub, GitLab, etc.) into canonical srcuri targets

Together, these enable "click to open in editor" functionality for code references shared anywhere on the web.

### Shared Target Model

All entry points converge on a single logical structure that Sorcery Desktop understands:

```
Target {
    remote: Option<String>,   // canonical repo identity (github.com/owner/repo)
    repo_name: String,        // workspace hint for display / matching
    ref_value: Option<String>,// branch/tag/SHA
    file_path: Option<String>,
    line: Option<u32>,
    column: Option<u32>
}
```

Workspace mirror links encode the target directly in the URL path/query. Provider passthrough links first parse the remote provider URL (client-side if a fragment is involved) and then synthesize the same target before dispatching `srcuri://...`.

---

## Mode Detection

```
Request to https://srcuri.com/...

┌────────────────────────────────────────────────────────┐
│ Path == "/" AND remote= present?                       │
├──────────┬─────────────────────────────────────────────┤
│   YES    │                    NO                       │
│          │                                             │
▼          ▼                                             │
Query      Is path a provider URL pattern?              │
Passthrough (github.com/..., /-/blob/..., /src/branch/)  │
           ├──────────┬──────────────────────────────────┤
           │   YES    │              NO                  │
           ▼          ▼                                  │
        Path-based   Path == "/"?                        │
        Passthrough  ├─────┬──────────────────────────┐  │
                     │ YES │           NO             │  │
                     ▼     ▼                          │  │
                  Landing  Workspace Mirror           │  │
                  Page     (workspace path)           │  │
└────────────────────────────────────────────────────────┘
```

### Provider Path Detection

The server detects provider URLs by checking for distinctive patterns:

| Pattern | Detected As |
|---------|-------------|
| Path starts with `github.com/` | GitHub |
| Path starts with `gitlab.com/` | GitLab |
| Path starts with `bitbucket.org/` | Bitbucket |
| Path starts with `codeberg.org/` | Codeberg |
| Path starts with `gitea.com/` | Gitea |
| Path contains `/_git/` | Azure DevOps |
| Path contains `/-/blob/` or `/-/tree/` | GitLab (self-hosted) |
| Path contains `/src/branch/` or `/src/tag/` | Gitea (self-hosted) |
| Path contains `/blob/` or `/tree/` | GitHub (self-hosted) |

---

## Workspace Mirror

### Purpose

Serve as an HTTPS "mirror" of `srcuri://` protocol URLs. When users share links in platforms that block custom protocols, they can share the HTTPS version instead.

### URL Format

```
https://srcuri.com/<workspace>/<path>[:<line>[:<column>]][?<query>]
```

Or via the `/open` endpoint with hash fragment:

```
https://srcuri.com/open#<workspace>/<path>[:<line>[:<column>]][?<query>]
```

### Components

| Component | Description | Example |
|-----------|-------------|---------|
| `workspace` | Repository/project identifier | `myrepo` |
| `path` | File path within workspace | `src/lib.rs` |
| `line` | Optional line number (1-indexed) | `42` |
| `column` | Optional column number (1-indexed, max 120) | `10` |
| `query` | Optional query params (`branch=`, `remote=`, etc.) | `branch=main` |

### Query Parameters

| Parameter | Description | Example |
|-----------|-------------|---------|
| `branch` | Git branch, tag, or commit ref | `main`, `v1.0.0`, `abc123` |
| `remote` | Remote repository URL (for clone-on-demand) | `https://github.com/owner/repo` |
| `workspace` | Explicit workspace name (overrides path) | `my-workspace` |

**Note on `remote=` format:** The preferred format includes the `https://` prefix
(e.g., `remote=https://github.com/owner/repo`) for git clone compatibility. However,
the server accepts both formats and normalizes internally:
- `remote=https://github.com/owner/repo` ✓ (preferred)
- `remote=github.com/owner/repo` ✓ (also accepted)

### Behavior

1. Server returns HTML page with embedded JavaScript
2. JavaScript extracts path, line, column from URL
3. JavaScript constructs `srcuri://` protocol URL
4. Browser redirects to protocol URL
5. If protocol handler not installed, show error with install link

### Line/Column Parsing (Client-Side)

The client JavaScript parses line numbers using right-to-left extraction:

```javascript
// Colon format: path:line:column
"src/lib.rs:42:10" → { path: "src/lib.rs", line: 42, column: 10 }

// GitHub format: path#L<n>
"src/lib.rs#L42" → { path: "src/lib.rs", line: 42 }

// Range format (takes first): path#L<n>-L<m>
"src/lib.rs#L10-L20" → { path: "src/lib.rs", line: 10 }
```

### Protocol URL Construction

```
srcuri://<workspace>/<path>:<line>:<column>?<query>
```

For absolute paths (triple slash):
```
srcuri:///<absolute-path>:<line>:<column>
```

### Example Flow

```
User clicks:    https://srcuri.com/myrepo/src/lib.rs:42?branch=main
Server returns: HTML with JavaScript
JS constructs:  srcuri://myrepo/src/lib.rs:42?branch=main
Browser opens:  Protocol handler (Sorcery Desktop)
Desktop:        Opens file in configured editor at line 42
```

### OpenGraph Meta Tags

Mirror pages include OpenGraph tags for Slack/Teams/Discord unfurling:

```html
<meta property="og:title" content="src/lib.rs:42 - myrepo">
<meta property="og:description" content="Open in editor: src/lib.rs at line 42">
<meta property="og:type" content="website">
<meta property="og:site_name" content="Sorcery">
<meta name="twitter:card" content="summary">
```

This ensures code links shared in chat apps display meaningful previews before clicking.

---

## Provider Passthrough

### Purpose

Convert web repository URLs (GitHub, GitLab, Bitbucket, Gitea, Codeberg, Azure DevOps) into canonical srcuri targets. This is the "viral on-ramp" for new users.

### URL Formats

**Path-based (recommended - shortest):**
```
https://srcuri.com/<provider-host>/<provider-path>[#fragment]
```

Examples:
- `srcuri.com/github.com/owner/repo/blob/main/file.rs#L42`
- `srcuri.com/gitlab.com/group/project/-/blob/main/file.py#L10`
- `srcuri.com/code.company.com/team/app/-/blob/dev/main.go#L100`

**Query-based (escape hatch):**
```
https://srcuri.com/?remote=<provider-url-encoded>
```

The path form is what we teach users (zero friction). The query form remains available for scripts or situations where the remote URL is already URL-encoded.

### Fragment Handling (Path-Based)

Browsers never send `#fragment` data to servers, so path-based provider requests must be served via an HTML+JS interstitial (`templates/provider.html`). That page:

1. Reads `window.location.pathname` and `window.location.hash`
2. Detects the provider + parses the remote URL client-side (including line numbers encoded as fragments)
3. Builds the canonical `srcuri://` URL (`srcuri://repo/file.rs:line?...`)
4. Attempts to open the protocol handler, showing fallback/install instructions if Sorcery Desktop is absent

This design keeps user-facing URLs clean (no `:line` suffixes) and still allows Slack/Teams unfurls to display the original provider URL.

### Behavior (Query-Based)

When `/?remote=` is present, the fragment is already URL-encoded, so the server can parse the provider URL directly:

1. Extract the `remote` query parameter
2. Parse host/repo/ref/file/line using the shared Target logic
3. Emit a 302 redirect to the equivalent workspace mirror URL (`/repo/path:line?...`)
4. The mirror page then issues the JS redirect to `srcuri://...`

This route is the documented escape hatch from the unified protocol packet and should remain functional for automation-heavy flows.

### Provider Detection

Detection uses URL patterns (not just hostname) to support self-hosted instances:

| Provider | Detection Pattern |
|----------|------------------|
| GitHub | `/blob/`, `/tree/`, `/blame/`, `/raw/` in path |
| GitHub | `github.dev` host (VS Code in browser) |
| GitHub | `codespaces.new` host or `/codespaces/` path |
| GitLab | `/-/blob/`, `/-/tree/`, `/-/blame/`, `/-/raw/` in path |
| GitLab | `/-/ide/project/` in path (Web IDE) |
| Bitbucket | `bitbucket.org` host or `/src/` pattern |
| Gitea/Forgejo | `/src/branch/`, `/src/tag/`, `/src/commit/` in path |
| Codeberg | `codeberg.org` host with Gitea patterns |
| Azure DevOps | `/_git/` in path |

Fallback: Check known canonical hosts (`github.com`, `gitlab.com`, etc.)

### Parsed Target Structure

```
SrcuriTarget {
    remote: String,           // "github.com/owner/repo"
    repo_name: String,        // "repo"
    ref_value: Option<String>,// branch/tag/SHA
    file_path: Option<String>,// "src/lib.rs"
    line: Option<u32>,        // 42
    is_absolute: bool,        // true for srcuri:///path (triple slash)
}
```

The `is_absolute` field indicates whether the path is an absolute filesystem path
(triple slash format: `srcuri:///etc/hosts`) rather than a workspace-relative path.
Absolute paths bypass workspace resolution on the desktop client.

### Provider URL Patterns

#### GitHub

```
https://github.com/:owner/:repo[/(blob|tree|blame|raw)/:ref[/:path]][#L<n>[-L<m>]]
```

Examples:
- `github.com/owner/repo` → repo only
- `github.com/owner/repo/blob/main/file.rs#L42` → full file with line

**GitHub.dev (VS Code in browser):**
```
https://github.dev/:owner/:repo/blob/:ref/:path
```
Treated identically to github.com URLs.

**GitHub Codespaces:**
```
https://codespaces.new/:owner/:repo
https://github.com/codespaces/new/:owner/:repo
```
Extracts owner/repo; file context not available from landing page URLs.

#### GitLab

```
https://gitlab.com/:group/:project[/-/(blob|tree|blame|raw)/:ref[/:path]][#L<n>]
```

Self-hosted detected by `/-/blob/` pattern.

**GitLab Web IDE:**
```
https://gitlab.com/-/ide/project/:group/:project/edit/:ref/-/:path
```
Extracts group, project, ref, and file path from IDE URLs.

#### Bitbucket

```
https://bitbucket.org/:workspace/:repo[/src/:ref[/:path]][#lines-<n>[:<m>]]
```

Line format: `#lines-5` or `#lines-5:10`

#### Gitea / Codeberg

```
https://gitea.com/:owner/:repo[/src/(branch|tag|commit)/:ref[/:path]][#L<n>]
```

Self-hosted detected by `/src/branch/`, `/src/tag/`, `/src/commit/` patterns.

#### Azure DevOps

```
https://dev.azure.com/:org[/:project]/_git/:repo[?path=/:path&version=(GB|GT|GC)<ref>&line=<n>]
```

Version prefixes:
- `GB` = branch (e.g., `GBmain`)
- `GT` = tag (e.g., `GTv1.0.0`)
- `GC` = commit (e.g., `GCabc123`)

### Output Mirror URL

```
https://srcuri.com/<repo_name>[/<file_path>][:<line>]?[branch=<ref>&]remote=https://<remote>
```

Examples:

| Input | Output |
|-------|--------|
| `github.com/owner/repo` | `/repo?remote=https://github.com/owner/repo` |
| `github.com/owner/repo/tree/main` | `/repo?branch=main&remote=https://github.com/owner/repo` |
| `github.com/owner/repo/blob/main/src/lib.rs#L42` | `/repo/src/lib.rs:42?branch=main&remote=https://github.com/owner/repo` |

### Error Handling

| Condition | Response |
|-----------|----------|
| Missing `remote` param (at root) | Show landing page |
| Invalid URL format | Show error page with original URL |
| Unrecognized provider | Show error page with original URL |
| Parse error | Show error page with original URL |

Error page must display the original URL so users can diagnose the issue.

---

## HTTP Responses

### Successful Translation (302)

```http
HTTP/1.1 302 Found
Location: /repo/src/lib.rs:42?branch=main&remote=https://github.com/owner/repo
```

### Workspace Mirror Page (200)

```http
HTTP/1.1 200 OK
Content-Type: text/html

<!DOCTYPE html>
<html>
  <!-- Page with JavaScript that redirects to srcuri:// -->
</html>
```

### Error Page (200)

```http
HTTP/1.1 200 OK
Content-Type: text/html

<!DOCTYPE html>
<html>
  <!-- Error message with original URL displayed -->
</html>
```

### Landing Page (200)

```http
HTTP/1.1 200 OK
Content-Type: text/html

<!DOCTYPE html>
<html>
  <!-- Instructions for using provider passthrough -->
</html>
```

---

## CORS

The server should allow cross-origin requests:

```http
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: GET, OPTIONS
Access-Control-Allow-Headers: *
```

---

## Well-Known Endpoint

```
GET /.well-known/srcuri.json
```

Returns protocol discovery information for clients.

---

## Health Check

```
GET /health
```

Returns `200 OK` with body `OK` for load balancer health checks.

---

## Implementation Checklist

### Required

- [ ] Workspace mirror: `/open` endpoint with client-side redirect
- [ ] Workspace mirror: Path-based URLs (`/<workspace>/<path>...`)
- [ ] Provider passthrough: Query-based (`/?remote=<url>`)
- [ ] Provider passthrough: Path-based (`/<provider-url>[:line]`)
- [ ] Path-based line suffix: Extract `:N` from end of path
- [ ] Provider detection: Pattern-based (supports self-hosted)
- [ ] Provider parsing: GitHub, GitLab, Bitbucket, Gitea, Codeberg, Azure DevOps
- [ ] Line number extraction for all fragment formats
- [ ] OpenGraph meta tags for Slack/Teams unfurling
- [ ] Error page showing original URL
- [ ] Landing page for root without `remote=`
- [ ] 302 redirects for successful translation
- [ ] CORS headers

### Recommended

- [ ] Health check endpoint
- [ ] Well-known endpoint
- [ ] Structured logging
- [ ] Request tracing

---

## Test Cases

Implementations should verify:

1. **Each provider** parses correctly with: repo-only, with-ref, with-file, with-line
2. **Self-hosted** GitLab and Gitea detected by URL pattern
3. **Line formats**: `#L42`, `#L10-L20`, `#lines-5`, `#lines-5:10`, `?line=12`
4. **Path suffix line**: `:42` suffix extracted correctly from path-based URLs
5. **Azure prefixes**: `GB`, `GT`, `GC` stripped correctly
6. **Mirror URL generation** includes correct query params
7. **Error cases**: invalid URL, unknown provider, malformed input
8. **Redirects** return 302 with correct Location header
9. **Path-based URLs**: Provider detection works without `https://` prefix
10. **Path normalization**: Leading slashes stripped, `https://` prefix handled

---

## Security Considerations

1. **URL validation**: Parse with a proper URL library, reject malformed input
2. **HTML escaping**: Escape all user input displayed in error pages
3. **No server-side git operations**: Passthrough only manipulates strings
4. **Rate limiting**: Consider rate limiting for production deployments
5. **Input length limits**: Reject excessively long URLs

---

## Reference Implementation

The reference implementation is in Rust using Axum:

- Repository: `sorcery-server`
- Key files:
  - `src/routes/passthrough.rs` - HTTP handlers for passthrough and mirror pages
  - `src/routes/provider.rs` - Provider passthrough interstitial pages
  - `src/static/app.js` - Client-side redirect for `/open` endpoint

**Shared Library:** The URL parsing logic lives in the `srcuri-core` crate
(located in `sorcery-desktop/srcuri-core/`). This ensures consistent parsing
between the server and desktop client. The server imports it as a dependency.

---

## Client-Side vs Server-Side Parsing

### The URL Fragment Problem

URL fragments (the `#L42` portion of URLs) are **never sent to servers** by browsers.
This is a fundamental browser security feature, not something we can work around.

```
User enters:  srcuri.com/github.com/owner/repo/blob/main/file.rs#L42
                                                                 ^^^^
Server sees:  srcuri.com/github.com/owner/repo/blob/main/file.rs
                                                          (fragment stripped)
```

### Why Two Parsers Exist

This creates an architectural requirement for **two URL parsing implementations**:

| Flow | Input Example | Parser Location | Reason |
|------|---------------|-----------------|--------|
| Path-based passthrough | `srcuri.com/github.com/.../file.rs#L42` | JavaScript (browser) | Server cannot see `#L42` |
| Query-based passthrough | `srcuri.com/?remote=...%23L42` | Rust (srcuri-core) | `#` is URL-encoded as `%23`, server sees it |
| Mirror mode | `srcuri.com/repo/file.rs:42?branch=main` | Neither | Line is in path (`:42`), not fragment |

The JavaScript in `provider.html` (~450 lines) reads `window.location.hash` to extract
line numbers from fragments, then constructs the `srcuri://` protocol URL client-side.

### Implications

1. **OpenGraph unfurling cannot include line numbers** for path-based URLs (Slack/Teams
   previews won't show "line 42" because the server never sees it)
2. **Two implementations must stay in sync** - provider URL parsing exists in both
   `srcuri-core` (Rust) and `provider.html` (JavaScript)
3. **Testing requires both paths** - server-side and client-side parsing

### Future Direction: WebAssembly

A future improvement would be to compile `srcuri-core` to **WebAssembly (WASM)** and
use it in the browser. This would:

- Eliminate the duplicated JavaScript parsing logic
- Ensure a single source of truth for URL parsing
- Reduce the risk of the two implementations diverging

This is tracked as a potential enhancement but not urgent, as the current dual-parser
approach is stable and well-tested.

---

## Mirror Page Features

### View on Provider Button

Mirror pages include a "View on [Provider]" button that links back to the
original source on GitHub/GitLab/etc. This is constructed from the parsed
`SrcuriTarget`:

```
SrcuriTarget.to_view_url() → "https://github.com/owner/repo/blob/main/file.rs#L42"
```

Provider-specific URL construction:
- **GitHub**: `https://{remote}/blob/{ref}/{path}#L{line}`
- **GitLab**: `https://{remote}/-/blob/{ref}/{path}#L{line}`
- **Bitbucket**: `https://{remote}/src/{ref}/{path}#lines-{line}`
- **Codeberg/Gitea**: `https://{remote}/src/branch/{ref}/{path}#L{line}`

This allows users to quickly navigate to the web view if they don't have
the Sorcery Desktop client installed.
