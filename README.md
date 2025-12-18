# Sorcery Server

Web gateway for [Sorcery Desktop](https://github.com/sorcery-app/sorcery-desktop) that bridges HTTPS URLs to the local `srcuri://` protocol handler.

## Overview

Sorcery Server enables srcuri links to work in web contexts (Jira, Slack, Teams, web browsers) where custom protocols face limitations. It provides public-facing HTTPS endpoints that redirect to the local srcuri protocol handler.

## Two Modes of Operation

The server operates on two subdomains with distinct purposes:

| Path Format | Purpose | User Action |
|-------------|---------|-------------|
| `srcuri.com/<provider-url>` | **Remote URL Translator** | Prefix GitHub/GitLab/etc. URLs with `https://srcuri.com/` |
| `srcuri.com/<workspace-path>` | **Direct Protocol Gateway** | Replace `srcuri://` with `https://srcuri.com/` for direct protocol links |

## Quick Start

### Opening GitHub/GitLab Links in Your Editor

**Prefix provider URLs with `https://srcuri.com/`**:

```
Before: https://github.com/owner/repo/blob/main/src/lib.rs#L42
After:  https://srcuri.com/github.com/owner/repo/blob/main/src/lib.rs#L42
        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The path-based translator:
- Parses GitHub, GitLab, Bitbucket, Gitea, Codeberg, and Azure DevOps URLs
- Preserves line numbers from `#L42` fragments (handled client-side)
- Redirects to `srcuri://` protocol to open in your editor

### Direct Protocol Links

**Replace `srcuri://` with `https://srcuri.com/`**:

```
Before: srcuri://myrepo/src/lib.rs:42?branch=main
After:  https://srcuri.com/myrepo/src/lib.rs:42?branch=main
```

## URL Formats

### Provider Passthrough

Supports all major Git hosting providers:

```bash
# GitHub
https://srcuri.com/github.com/owner/repo/blob/main/file.rs#L42

# GitLab
https://srcuri.com/gitlab.com/group/project/-/blob/master/file.py#L10

# Self-hosted GitLab
https://srcuri.com/gitlab.mycompany.com/team/app/-/blob/dev/main.go#L100

# Bitbucket
https://srcuri.com/bitbucket.org/workspace/repo/src/main/file.py#lines-5

# Gitea/Codeberg
https://srcuri.com/codeberg.org/user/repo/src/branch/main/file.go#L24

# Azure DevOps
https://srcuri.com/dev.azure.com/org/project/_git/repo?path=/src/file.ts&line=12

# Optional escape hatch (URL-encoded provider URL)
https://srcuri.com/?remote=https://github.com/owner/repo/blob/main/file.rs#L42
```

### srcuri.com - Direct Protocol Gateway

For sharing direct srcuri links:

```bash
# Workspace-relative paths
https://srcuri.com/myrepo/src/lib.rs:42?branch=main&remote=https://github.com/owner/myrepo

# With remote for cloning
https://srcuri.com/myrepo/src/lib.rs:42?branch=main&remote=https://github.com/owner/myrepo
```

### Enterprise Subdomains

Enterprise tenants get their own subdomain:

```
https://acme.srcuri.com/internal-tools/src/auth.rs:42
```

## Features

- **Provider Passthrough** (`srcuri.com/<provider-url>`) - Convert GitHub/GitLab URLs to srcuri links
- **Direct Protocol Gateway** (`srcuri.com`) - 1:1 mapping from srcuri:// to https://
- **Enterprise Subdomains** - Multi-tenant support for organizations
- **Line Number Preservation** - Client-side JS preserves `#L42` fragments
- **Link Unfurling** - OpenGraph tags for Slack/Teams previews
- **www Redirect** - `www.srcuri.com` redirects to `srcuri.com`

## Development

### Prerequisites

- Rust 1.75+
- Cargo

### Running Locally

```bash
cargo run
```

The server starts on `http://localhost:3000`.

### Testing Subdomains Locally

Use the `?_subdomain=` query parameter to simulate subdomains:

```bash
# Test enterprise tenant
curl "http://localhost:3000/myrepo/file.rs:42?_subdomain=acme"

# Test www redirect
curl "http://localhost:3000/?_subdomain=www"
```

### Environment Variables

- `PORT` - Server port (default: 3000)
- `TENANTS_DIR` - Directory containing tenant configs (default: `sorcery-server/tenants`)
- `RUST_LOG` - Logging level (default: `sorcery_server=debug`)

### Testing

```bash
cargo test
```

111 tests covering URL parsing, subdomain detection, and integration scenarios.

### Test Endpoints

```bash
# Health check
curl http://localhost:3000/health

# Tenant config
curl http://localhost:3000/.well-known/srcuri.json

# Direct protocol mode (root)
curl http://localhost:3000/

# Provider passthrough
curl "http://localhost:3000/github.com/owner/repo/blob/main/file.rs"
```

## Deployment

### Docker

```bash
docker build -t sorcery-server .
docker run -p 8080:8080 -e PORT=8080 sorcery-server
```

### DNS Configuration

```
srcuri.com       → A/CNAME to server
*.srcuri.com     → A/CNAME to server (enterprise subdomains)
```

### Production Deployment

Compatible with: Fly.io, Railway, AWS ECS, Google Cloud Run, or any Docker host.

## Architecture

### Subdomain Routing

```
Request with Host header
    ↓
Subdomain Detection
    ↓
┌─────────────────────────────────────────┐
│ www.srcuri.com   → 301 to srcuri.com    │
│ srcuri.com       → Direct Protocol      │
│ acme.srcuri.com  → Enterprise Tenant    │
└─────────────────────────────────────────┘
```

### Provider Passthrough Flow

```
User shares: https://srcuri.com/github.com/owner/repo/blob/main/file.rs#L42
    ↓
Server returns: HTML page with JavaScript
    ↓
Browser JS reads: window.location (including #L42 fragment)
    ↓
JS parses: GitHub URL → extracts repo, branch, file, line
    ↓
JS builds: srcuri://repo/file.rs:42?branch=main&remote=https://github.com/owner/repo
    ↓
JS redirects: window.location.href = srcuri://...
    ↓
OS launches: Sorcery Desktop
    ↓
Editor opens: file.rs at line 42
```

### srcuri.com Flow (Direct Protocol)

```
User shares: https://srcuri.com/repo/src/lib.rs:42?branch=main
    ↓
Server returns: HTML with OpenGraph tags + JavaScript
    ↓
JS redirects: srcuri://repo/src/lib.rs:42?branch=main
    ↓
OS launches: Sorcery Desktop
```

### Why Fragments Are Handled Client-Side

URL fragments (`#L42`) are never sent to servers - this is standard browser behavior. The provider passthrough uses client-side JavaScript to read the fragment and include line numbers in the srcuri:// redirect.

This means:
- **OG unfurling** cannot include line numbers (Slack/Teams only see the server response)
- **Line numbers work** because JavaScript preserves them client-side

## Tenant Configuration

Create tenant config files in `tenants/` directory:

```json
{
  "tenant_id": "acme",
  "require_auth": false,
  "allowed_paths": ["src/**", "lib/**"],
  "fallback_viewer_url": "https://github.com/acme/repo/blob/main/{path}#L{line}"
}
```

Subdomain mapping:
- `srcuri.com` → `tenants/default.json`
- `acme.srcuri.com` → `tenants/acme.json`

## Security

- **Fragment-based privacy**: For `/open` endpoint, file paths stay in browser
- **Client-side parsing**: Sensitive data processed in JavaScript
- **Local validation**: srcuri client validates paths before opening
- **CORS enabled**: Allows cross-origin requests for embedding

## Documentation

- **URL Design Rationale**: `dev/srcuri-website-url-design.md`
- **Translator Mode Spec**: `dev/translator-mode.md`
- **Server Spec**: `dev/server-spec.md`

## Repository

[github.com/sorcery-app/sorcery-server](https://github.com/sorcery-app/sorcery-server)

## License

AGPL-3.0 License
