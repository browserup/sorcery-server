use askama::Template;
use axum::{
    extract::Query,
    http::{Uri, header, HeaderValue},
    response::{Html, Redirect, IntoResponse, Response},
};
use serde::Deserialize;
use std::fmt::Write;
use crate::parsing::{parse_remote_url, extract_path_line_suffix, ParseError, SrcuriTarget};
use super::templates::{MirrorTemplate, ErrorTemplate};

/// URL mode based on authority (first path segment)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UrlMode {
    /// srcuri.com/myrepo/... → srcuri://myrepo/...
    ImplicitWorkspace,
    /// srcuri.com/wks/myrepo/... → srcuri://wks/myrepo/...
    ExplicitWorkspace,
    /// srcuri.com/rel/... → srcuri://rel/...
    Relative,
    /// srcuri.com/abs/... → srcuri://abs/...
    Absolute,
    /// srcuri.com/ext/https/... → srcuri://ext/https/...
    External,
}

/// Reserved authority tokens that cannot be used as workspace names
const RESERVED_AUTHORITIES: [&str; 4] = ["wks", "rel", "abs", "ext"];

/// Detect URL mode from the first path segment
fn detect_url_mode(path: &str) -> (UrlMode, &str) {
    let normalized = path.trim_start_matches('/');

    // Get first segment
    let first_segment = normalized.split('/').next().unwrap_or("");

    match first_segment {
        "wks" => {
            let rest = normalized.strip_prefix("wks/").unwrap_or(normalized);
            (UrlMode::ExplicitWorkspace, rest)
        }
        "rel" => {
            let rest = normalized.strip_prefix("rel/").unwrap_or(normalized);
            (UrlMode::Relative, rest)
        }
        "abs" => {
            let rest = normalized.strip_prefix("abs/").unwrap_or(normalized);
            (UrlMode::Absolute, rest)
        }
        "ext" => {
            let rest = normalized.strip_prefix("ext/").unwrap_or(normalized);
            (UrlMode::External, rest)
        }
        _ => (UrlMode::ImplicitWorkspace, normalized),
    }
}

/// Sanitize URL for use in href attribute - only allow http/https protocols
/// Blocks javascript:, data:, vbscript: and other dangerous protocols
fn safe_href_url(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        url.to_string()
    } else {
        String::new()
    }
}

/// Validate branch names - allows chars found in real GitHub branch names
fn is_valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | '/' | '@' | ',' | '(' | ')' | '+' | '#' | '=')
        })
        && !name.starts_with('/')
        && !name.ends_with('/')
        && !name.contains("..")
}

/// Validate remote URL structure
fn is_valid_remote_url(url: &str) -> bool {
    let path = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("git@");

    !path.is_empty()
        && path.len() <= 256
        && path.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@')
        })
        && !path.contains("..")
        && !path.contains("//")
        && !path.starts_with('/')
}

/// Validate workspace/repo names - project name format
fn is_valid_workspace_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
        })
        // Reserved authorities cannot be used as workspace names
        && !RESERVED_AUTHORITIES.contains(&name.to_lowercase().as_str())
}

/// Validate file paths - safe characters only, no shell metacharacters.
/// Allows: alphanumeric, standard path chars (-_./), space, @ (npm scopes), + (C++ files),
/// parentheses, and square brackets. A leading '~' is permitted for home-relative paths.
/// For Windows paths, allows ':' after drive letter (e.g., C:/Users/...).
fn is_valid_file_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 1024 {
        return false;
    }

    // Check for Windows drive letter pattern (X:/ where X is A-Z)
    let is_windows_path = path.len() >= 3
        && path.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
        && path.chars().nth(1) == Some(':')
        && path.chars().nth(2) == Some('/');

    for (idx, ch) in path.chars().enumerate() {
        if idx == 0 && ch == '~' {
            continue;
        }

        if ch == '~' {
            return false;
        }

        // Allow ':' at position 1 for Windows drive letters
        if is_windows_path && idx == 1 && ch == ':' {
            continue;
        }

        if ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '_' | '.' | '/' | ' ' | '@' | '+' | '(' | ')' | '[' | ']')
        {
            continue;
        }

        return false;
    }

    !path.contains("..")
}

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
/// Detects URL mode from authority and routes accordingly
pub async fn catchall_handler(
    uri: Uri,
    Query(params): Query<MirrorQuery>,
) -> Response {
    let path = uri.path().to_string();

    // Detect URL mode from first path segment
    let (mode, _rest_path) = detect_url_mode(&path);

    match mode {
        UrlMode::External => {
            // External mode: srcuri.com/ext/https/github.com/...
            // Serve provider interstitial page (client-side to preserve fragments)
            serve_provider_page()
        }
        UrlMode::Absolute | UrlMode::Relative | UrlMode::ExplicitWorkspace | UrlMode::ImplicitWorkspace => {
            // Check if implicit workspace path looks like a provider URL
            if mode == UrlMode::ImplicitWorkspace && is_provider_path(&path) {
                serve_provider_page()
            } else {
                // Serve mirror page with appropriate mode
                serve_mirror_page(&path, mode, params).into_response()
            }
        }
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
fn serve_mirror_page(path: &str, mode: UrlMode, params: MirrorQuery) -> Response {
    // URL-decode the path (converts %20 to space, %5B to [, etc.)
    let decoded_path = percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .into_owned();

    // Validate branch name if provided
    if let Some(ref branch) = params.branch {
        if !is_valid_branch_name(branch) {
            return render_invalid_ref_error("branch", branch);
        }
    }
    // Validate remote URL if provided
    if let Some(ref remote) = params.remote {
        if !is_valid_remote_url(remote) {
            return render_invalid_param_error("remote", remote);
        }
    }
    let target = parse_mirror_path(&decoded_path, mode, params);
    // Validate extracted repo name (workspace) - only for workspace modes
    if !target.repo_name.is_empty()
        && matches!(mode, UrlMode::ImplicitWorkspace | UrlMode::ExplicitWorkspace)
        && !is_valid_workspace_name(&target.repo_name)
    {
        return render_invalid_param_error("workspace", &target.repo_name);
    }
    // Validate file path (length limit, path traversal)
    if let Some(ref file_path) = target.file_path {
        if !is_valid_file_path(file_path) {
            return render_invalid_param_error("path", file_path);
        }
    }
    render_mirror_page_with_mode(&target, mode)
}

fn render_invalid_ref_error(param_type: &str, ref_name: &str) -> Response {
    let safe_display: String = ref_name
        .chars()
        .take(100)
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '@' | ',' | '(' | ')' | '+' | '#' | '=' | ' ') { c } else { '?' })
        .collect();

    let allowed_chars = match param_type {
        "branch" => "letters, numbers, and - _ . / @ , ( ) + # =",
        "tag" => "letters, numbers, and - _ . / +",
        _ => "letters, numbers, and - _ . / @ , ( ) + # =",
    };

    let template = ErrorTemplate {
        message: format!(
            "Invalid {} name: \"{}\". {} names may only contain {}",
            param_type, safe_display, param_type, allowed_chars
        ),
        url: String::new(),
    };
    Html(template.render().unwrap_or_else(|e| format!("Template error: {}", e))).into_response()
}

fn render_invalid_param_error(param_type: &str, value: &str) -> Response {
    let safe_display: String = value
        .chars()
        .take(100)
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@' | ' ') { c } else { '?' })
        .collect();

    let message = match param_type {
        "remote" => format!(
            "Invalid remote URL: \"{}\". Remote URLs may only contain letters, numbers, and - _ . / : @",
            safe_display
        ),
        "workspace" => format!(
            "Invalid workspace name: \"{}\". Workspace names may only contain letters, numbers, and - _ .",
            safe_display
        ),
        "commit" => format!(
            "Invalid commit SHA: \"{}\". Commit SHAs must be 7-64 hexadecimal characters (0-9, a-f)",
            safe_display
        ),
        "path" => format!(
            "Invalid file path: \"{}\". Paths may only contain letters, numbers, and - _ . / @ + (space) ( ) [ ] with an optional leading ~",
            safe_display
        ),
        _ => format!("Invalid {}: \"{}\"", param_type, safe_display),
    };

    let template = ErrorTemplate {
        message,
        url: String::new(),
    };
    Html(template.render().unwrap_or_else(|e| format!("Template error: {}", e))).into_response()
}

/// Parse a mirror mode path based on detected URL mode
fn parse_mirror_path(path: &str, mode: UrlMode, params: MirrorQuery) -> SrcuriTarget {
    let trimmed = path.trim_start_matches('/');

    // Strip authority prefix if present
    let clean_path = match mode {
        UrlMode::ExplicitWorkspace => trimmed.strip_prefix("wks/").unwrap_or(trimmed),
        UrlMode::Relative => trimmed.strip_prefix("rel/").unwrap_or(trimmed),
        UrlMode::Absolute => trimmed.strip_prefix("abs/").unwrap_or(trimmed),
        UrlMode::External => trimmed.strip_prefix("ext/").unwrap_or(trimmed),
        UrlMode::ImplicitWorkspace => trimmed,
    };

    // Extract line number from :N suffix
    let (path_without_line, line) = extract_path_line_suffix(clean_path);

    // Normalize remote (strip https:// if present, accept both formats)
    let remote = normalize_remote(params.remote);

    match mode {
        UrlMode::Absolute => {
            // Absolute path: no workspace, full path goes in file_path
            SrcuriTarget {
                remote,
                repo_name: String::new(),
                ref_value: params.branch,
                file_path: Some(path_without_line.to_string()),
                line,
                is_absolute: true,
            }
        }
        UrlMode::Relative => {
            // Relative mode: path is a search pattern, no workspace extraction
            SrcuriTarget {
                remote,
                repo_name: String::new(),
                ref_value: params.branch,
                file_path: Some(path_without_line.to_string()),
                line,
                is_absolute: false,
            }
        }
        UrlMode::External => {
            // External mode: path is provider URL (https/github.com/...)
            SrcuriTarget {
                remote,
                repo_name: String::new(),
                ref_value: params.branch,
                file_path: Some(path_without_line.to_string()),
                line,
                is_absolute: false,
            }
        }
        UrlMode::ImplicitWorkspace | UrlMode::ExplicitWorkspace => {
            // Workspace modes: split into workspace/repo and file path
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
}

fn render_mirror_page_with_mode(target: &SrcuriTarget, mode: UrlMode) -> Response {
    // Build srcuri:// URL with authority-based mode detection (v1 spec)
    let mut srcuri = match mode {
        UrlMode::Absolute => {
            // Absolute path: srcuri://abs/path/to/file
            let path = target.file_path.as_deref().unwrap_or("");
            format!("srcuri://abs/{}", path)
        }
        UrlMode::Relative => {
            // Relative mode: srcuri://rel/path/to/file
            let path = target.file_path.as_deref().unwrap_or("");
            format!("srcuri://rel/{}", path)
        }
        UrlMode::External => {
            // External mode: srcuri://ext/path (preserved from URL)
            let path = target.file_path.as_deref().unwrap_or("");
            format!("srcuri://ext/{}", path)
        }
        UrlMode::ExplicitWorkspace => {
            // Explicit workspace: srcuri://wks/repo/path
            let mut s = format!("srcuri://wks/{}", target.repo_name);
            if let Some(ref path) = target.file_path {
                s.push('/');
                s.push_str(path);
            }
            s
        }
        UrlMode::ImplicitWorkspace => {
            // Implicit workspace: srcuri://repo/path (authority IS workspace)
            let mut s = format!("srcuri://{}", target.repo_name);
            if let Some(ref path) = target.file_path {
                s.push('/');
                s.push_str(path);
            }
            s
        }
    };

    if let Some(line) = target.line {
        let _ = write!(srcuri, ":{}", line);
    }

    let mut query_parts = Vec::new();
    if let Some(ref branch) = target.ref_value {
        // URL-encode branch names to handle special characters like + # =
        let encoded: String = url::form_urlencoded::byte_serialize(branch.as_bytes()).collect();
        query_parts.push(format!("branch={}", encoded));
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
    } else if !target.repo_name.is_empty() {
        format!("{} repository", target.repo_name)
    } else {
        "Code reference".to_string()
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
        url: safe_href_url(&error.original_url),
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
