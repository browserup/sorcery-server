use askama::Template;
use axum::{
    extract::Query,
    http::{Uri, header, HeaderValue},
    response::{Html, Redirect, IntoResponse, Response},
};
use serde::Deserialize;
use crate::parsing::{parse_remote_url, extract_path_line_suffix, ParseError, SrcuriTarget};
use super::templates::{MirrorTemplate, ErrorTemplate};

#[derive(Deserialize)]
pub struct PassthroughQuery {
    pub remote: Option<String>,
}

#[derive(Deserialize)]
pub struct MirrorQuery {
    pub branch: Option<String>,
    pub remote: Option<String>,
}

/// Root handler: ?remote= for provider passthrough, else landing page
pub async fn root_handler(Query(params): Query<PassthroughQuery>) -> Response {
    match params.remote {
        Some(remote_url) => passthrough_redirect(&remote_url).into_response(),
        None => Html(include_str!("../templates/landing.html").to_string()).into_response(),
    }
}

/// Catch-all handler for path-based URLs
/// Detects whether path is a provider URL (passthrough) or workspace path (mirror)
pub async fn catchall_handler(
    uri: Uri,
    Query(params): Query<MirrorQuery>,
) -> Response {
    let path = uri.path().to_string();
    // Check if this looks like a provider URL - serve HTML+JS interstitial
    // (must be client-side to preserve URL fragments like #L42)
    if is_provider_path(&path) {
        serve_provider_page()
    } else {
        // It's a workspace mirror path - serve the mirror page
        serve_mirror_page(&path, params).into_response()
    }
}

/// Detect if path looks like a provider URL (github.com/..., gitlab.com/..., etc.)
fn is_provider_path(path: &str) -> bool {
    let normalized = path.trim_start_matches('/');

    // Check for https:// prefix (user included full URL)
    if normalized.starts_with("https://") || normalized.starts_with("http://") {
        return true;
    }

    // Check for known provider hostnames at start
    let provider_patterns = [
        "github.com/",
        "github.dev/",
        "codespaces.new/",
        "gitlab.com/",
        "bitbucket.org/",
        "gitea.com/",
        "codeberg.org/",
        "dev.azure.com/",
    ];

    for pattern in provider_patterns {
        if normalized.starts_with(pattern) {
            return true;
        }
    }

    // Check for provider URL patterns in path
    if normalized.contains("/-/blob/") || normalized.contains("/-/tree/") {
        return true; // GitLab-style
    }
    if normalized.contains("/-/ide/") {
        return true; // GitLab Web IDE
    }
    if normalized.contains("/codespaces/") {
        return true; // GitHub Codespaces
    }
    if normalized.contains("/src/branch/") || normalized.contains("/src/tag/") {
        return true; // Gitea-style
    }
    if normalized.contains("/_git/") {
        return true; // Azure DevOps
    }

    // Check if path segment looks like a hostname (contains dot before first slash)
    if let Some(first_segment) = normalized.split('/').next() {
        if first_segment.contains('.') && !first_segment.contains(':') {
            // Likely a hostname like gitlab.mycompany.com
            return true;
        }
    }

    false
}

fn serve_provider_page() -> Response {
    Html(include_str!("../templates/provider.html").to_string()).into_response()
}

/// For query-based passthrough (?remote=...), we can parse server-side
/// since the fragment is URL-encoded in the query parameter
fn passthrough_redirect(remote_url: &str) -> Response {
    match parse_remote_url(remote_url) {
        Ok(target) => {
            let mirror_url = target.to_mirror_url();
            Redirect::to(&mirror_url).into_response()
        }
        Err(e) => render_error(e).into_response(),
    }
}

/// Serve the mirror page for srcuri:// protocol redirect
fn serve_mirror_page(path: &str, params: MirrorQuery) -> Response {
    let target = parse_mirror_path(path, params);
    render_mirror_page(&target)
}

/// Parse a mirror mode path like "repo/src/lib.rs:42" or "//absolute/path.rs:42"
fn parse_mirror_path(path: &str, params: MirrorQuery) -> SrcuriTarget {
    // Check for absolute path: starts with // (after the initial / from URI)
    // e.g., URI path "///Users/foo/file.txt" arrives as "///Users/foo/file.txt"
    // We need 3 slashes total for absolute paths: first slash is the URI path separator,
    // next two indicate "absolute path" in srcuri:// protocol
    let trimmed_once = path.strip_prefix('/').unwrap_or(path);
    let is_absolute = trimmed_once.starts_with('/');

    let clean_path = if is_absolute {
        // Absolute path: keep one leading slash
        trimmed_once
    } else {
        // Workspace path: no leading slashes
        trimmed_once
    };

    // Extract line number from :N suffix
    let (path_without_line, line) = extract_path_line_suffix(clean_path);

    // Normalize remote (strip https:// if present, accept both formats)
    let remote = normalize_remote(params.remote);

    if is_absolute {
        // Absolute path: no workspace, full path goes in file_path
        SrcuriTarget {
            remote,
            repo_name: String::new(),
            ref_value: params.branch,
            file_path: Some(path_without_line.to_string()),
            line,
            is_absolute: true,
        }
    } else {
        // Split into workspace/repo and file path
        let parts: Vec<&str> = path_without_line.splitn(2, '/').collect();
        let repo_name = parts.first().unwrap_or(&"").to_string();
        let file_path = parts.get(1).map(|s| s.to_string());

        SrcuriTarget {
            remote,
            repo_name,
            ref_value: params.branch,
            file_path,
            line,
            is_absolute: false,
        }
    }
}

fn render_mirror_page(target: &SrcuriTarget) -> Response {
    // Build srcuri:// URL
    let mut srcuri = if target.is_absolute {
        // Absolute path: srcuri:///path/to/file
        let path = target.file_path.as_deref().unwrap_or("");
        format!("srcuri://{}", path)
    } else {
        // Workspace path: srcuri://workspace/path/to/file
        let mut s = format!("srcuri://{}/", target.repo_name);
        if let Some(ref path) = target.file_path {
            s.push_str(path);
        }
        s
    };
    if let Some(line) = target.line {
        srcuri.push_str(&format!(":{}", line));
    }

    let mut query_parts = Vec::new();
    if let Some(ref branch) = target.ref_value {
        query_parts.push(format!("branch={}", branch));
    }
    if !target.remote.is_empty() {
        // Always output with https:// prefix for git clone compatibility
        query_parts.push(format!("remote=https://{}", target.remote));
    }
    if !query_parts.is_empty() {
        srcuri.push('?');
        srcuri.push_str(&query_parts.join("&"));
    }

    // Build display info
    let display_path = target.file_path.as_deref().unwrap_or("");
    let display_line = target.line.map(|l| format!(":{}", l)).unwrap_or_default();
    let display_branch = target.ref_value.as_deref().unwrap_or("main");

    // Generate OG description
    let og_description = if !display_path.is_empty() {
        format!("{}{} on {} branch", display_path, display_line, display_branch)
    } else {
        format!("{} repository", target.repo_name)
    };

    // Generate view URL for remote provider (GitHub, GitLab, etc.)
    let view_url = target.to_view_url().unwrap_or_default();
    let provider_name = target.provider_name();

    let template = MirrorTemplate {
        srcuri_url: srcuri,
        repo_name: target.repo_name.clone(),
        file_path: display_path.to_string(),
        line: display_line,
        og_description,
        view_url,
        provider_name: provider_name.to_string(),
    };

    let html = template.render().unwrap_or_else(|e| {
        format!("Template error: {}", e)
    });

    let mut response = Html(html).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    response.headers_mut().insert(
        header::PRAGMA,
        HeaderValue::from_static("no-cache"),
    );
    response.headers_mut().insert(
        header::EXPIRES,
        HeaderValue::from_static("0"),
    );
    response
}

fn render_error(error: ParseError) -> Html<String> {
    let template = ErrorTemplate {
        message: error.message,
        url: error.original_url,
    };
    Html(template.render().unwrap_or_else(|e| {
        format!("Template error: {}", e)
    }))
}

/// Normalize remote URL to strip protocol prefix.
/// Accepts both "github.com/owner/repo" and "https://github.com/owner/repo".
/// Returns just "github.com/owner/repo" for consistent internal storage.
fn normalize_remote(remote: Option<String>) -> String {
    remote.map(|r| {
        r.trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string()
    }).unwrap_or_default()
}
