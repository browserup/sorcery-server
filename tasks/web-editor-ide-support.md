# Web Editor IDE URL Support

Add support for web-based IDE URLs from GitLab and GitHub.

**Status: COMPLETE** - All tests passing (102 unit tests, 8 integration tests)

## URL Patterns to Support

### GitLab Web IDE
```
https://gitlab.com/-/ide/project/paynearme/juno/merge_requests/5942
https://gitlab.com/-/ide/project/paynearme/juno/edit/main/-/Gemfile
```

**Pattern:** `/-/ide/project/:group/:project/...`
- `merge_requests/:mr_id` - MR context (repo only, no file)
- `edit/:ref/-/:path` - File editing context

### GitLab Standard (already supported, verify)
```
https://gitlab.com/paynearme/juno/-/blob/main/Gemfile?ref_type=heads
```
Already handled by existing `parse_gitlab()` - query params are ignored which is correct.

### GitHub.dev (VS Code in browser)
```
https://github.dev/ericbeland/enhanced_errors/blob/main/Gemfile
```

**Pattern:** Same as github.com but on `github.dev` domain
- `/:owner/:repo/blob/:ref/:path`

### GitHub Codespaces
```
https://github.com/codespaces/new/browserup/browserup-proxy/pull/382?resume=1
```

**Pattern:** `github.com/codespaces/new/:owner/:repo/pull/:pr_id`
- Creates new codespace for a PR - extract repo info only

---

## Implementation Plan

### 1. Update `detect_provider()` in parser.rs

- [x] Add `github.dev` host detection → Provider::GitHub
- [x] Add `codespaces.new` host detection → Provider::GitHub
- [x] Add `/-/ide/` path pattern detection → Provider::GitLab
- [x] Add `/codespaces/` path pattern detection → Provider::GitHub

```rust
// In detect_provider():

// GitHub.dev (VS Code in browser) - same patterns as github.com
if host == "github.dev" {
    return Some(Provider::GitHub);
}

// GitLab Web IDE
if path.starts_with("/-/ide/") {
    return Some(Provider::GitLab);
}

// GitHub Codespaces (on github.com)
if path.starts_with("/codespaces/") {
    return Some(Provider::GitHub);
}
```

### 2. Update `parse_github()` for Codespaces

- [x] Detect `codespaces.new/:owner/:repo` domain pattern
- [x] Detect `/codespaces/new/:owner/:repo/...` path pattern
- [x] Handle `/pull/:id` URLs (repo-only)
- [x] Extract owner/repo, return repo-only target

```rust
// Check for codespaces pattern: /codespaces/new/:owner/:repo/...
if segments.get(0) == Some(&"codespaces") && segments.get(1) == Some(&"new") {
    if segments.len() >= 4 {
        let owner = segments[2];
        let repo = segments[3];
        let remote = format!("{}/{}/{}", host, owner, repo);
        return Ok(SrcuriTarget {
            remote,
            repo_name: repo.to_string(),
            ref_value: None,
            file_path: None,
            line: None,
        });
    }
}
```

### 3. Update `parse_gitlab()` for Web IDE

- [x] Detect `/-/ide/project/:group/:project/...` pattern
- [x] Handle `edit/:ref/-/:path` sub-pattern for file paths
- [x] Handle `edit/:ref/-/` (no file, trailing slash)
- [x] Handle `edit/:ref` (no file, no separator)
- [x] Handle `edit/:ref/:path` (no -/ separator, file directly)
- [x] Handle `merge_requests/:id` sub-pattern (repo-only)

```rust
// Check for Web IDE pattern: /-/ide/project/:group/:project/...
if segments.get(0) == Some(&"-") && segments.get(1) == Some(&"ide")
   && segments.get(2) == Some(&"project") {
    if segments.len() >= 5 {
        let group = segments[3];
        let project = segments[4];
        let remote = format!("{}/{}/{}", host, group, project);

        // Check for edit/:ref/-/:path pattern
        if segments.get(5) == Some(&"edit") {
            let ref_value = segments.get(6).map(|s| s.to_string());
            // After "edit/:ref/-/", the rest is file path
            let dash_pos = segments.iter().skip(7).position(|&s| s == "-");
            let file_path = if let Some(pos) = dash_pos {
                let start = 7 + pos + 1;
                if segments.len() > start {
                    Some(segments[start..].join("/"))
                } else {
                    None
                }
            } else {
                None
            };

            return Ok(SrcuriTarget {
                remote,
                repo_name: project.to_string(),
                ref_value,
                file_path,
                line: extract_github_line(url.fragment()),
            });
        }

        // Other patterns (merge_requests, etc.) - repo only
        return Ok(SrcuriTarget {
            remote,
            repo_name: project.to_string(),
            ref_value: None,
            file_path: None,
            line: None,
        });
    }
}
```

### 4. Update `is_translatable_path()` in routes/translator.rs

- [x] Add `github.dev/` pattern
- [x] Add `codespaces.new/` pattern
- [x] Add `/-/ide/` pattern
- [x] Add `/codespaces/` pattern

```rust
// In is_translatable_path():

// GitHub.dev
if path_lower.contains("github.dev/") {
    return true;
}

// GitLab Web IDE
if path_lower.contains("/-/ide/") {
    return true;
}

// GitHub Codespaces
if path_lower.contains("/codespaces/new/") || path_lower.contains("/codespaces/") {
    return true;
}
```

### 5. Add Tests

- [x] `github_dev_blob` - github.dev file URL
- [x] `github_dev_tree` - github.dev directory URL
- [x] `github_dev_repo_root` - github.dev repo-only URL
- [x] `github_dev_deep_path` - github.dev deep file path
- [x] `github_dev_pull_request` - github.dev PR URL
- [x] `github_codespaces_pr` - codespaces PR URL
- [x] `codespaces_new_basic` - codespaces.new domain
- [x] `codespaces_new_with_query_params` - with ?quickstart=1
- [x] `gitlab_web_ide_edit_file` - GitLab IDE edit URL
- [x] `gitlab_web_ide_edit_nested_path` - deep file paths
- [x] `gitlab_web_ide_project_root_trailing_dash` - edit/master/-/
- [x] `gitlab_web_ide_no_trailing_dash` - edit/master (no file)
- [x] `gitlab_web_ide_no_dash_separator` - file without -/ separator
- [x] `gitlab_web_ide_mr` - GitLab IDE MR URL
- [x] Detection tests for all new patterns

---

## Test Cases

```rust
#[test]
fn github_dev_blob() {
    let result = parse_remote_url("https://github.dev/ericbeland/enhanced_errors/blob/main/Gemfile").unwrap();
    assert_eq!(result.remote, "github.dev/ericbeland/enhanced_errors");
    assert_eq!(result.repo_name, "enhanced_errors");
    assert_eq!(result.ref_value, Some("main".to_string()));
    assert_eq!(result.file_path, Some("Gemfile".to_string()));
}

#[test]
fn github_codespaces_pr() {
    let result = parse_remote_url("https://github.com/codespaces/new/browserup/browserup-proxy/pull/382?resume=1").unwrap();
    assert_eq!(result.remote, "github.com/browserup/browserup-proxy");
    assert_eq!(result.repo_name, "browserup-proxy");
    assert_eq!(result.ref_value, None);
    assert_eq!(result.file_path, None);
}

#[test]
fn gitlab_web_ide_edit() {
    let result = parse_remote_url("https://gitlab.com/-/ide/project/paynearme/juno/edit/main/-/Gemfile").unwrap();
    assert_eq!(result.remote, "gitlab.com/paynearme/juno");
    assert_eq!(result.repo_name, "juno");
    assert_eq!(result.ref_value, Some("main".to_string()));
    assert_eq!(result.file_path, Some("Gemfile".to_string()));
}

#[test]
fn gitlab_web_ide_mr() {
    let result = parse_remote_url("https://gitlab.com/-/ide/project/paynearme/juno/merge_requests/5942").unwrap();
    assert_eq!(result.remote, "gitlab.com/paynearme/juno");
    assert_eq!(result.repo_name, "juno");
    assert_eq!(result.ref_value, None);
    assert_eq!(result.file_path, None);
}

#[test]
fn detect_github_dev() {
    let url = Url::parse("https://github.dev/owner/repo/blob/main/f.rs").unwrap();
    assert_eq!(detect_provider(&url), Some(Provider::GitHub));
}

#[test]
fn detect_gitlab_web_ide() {
    let url = Url::parse("https://gitlab.com/-/ide/project/group/proj/edit/main/-/f.py").unwrap();
    assert_eq!(detect_provider(&url), Some(Provider::GitLab));
}
```

---

## Files to Modify

1. `src/translator/parser.rs`
   - `detect_provider()` - add new patterns
   - `parse_github()` - handle codespaces
   - `parse_gitlab()` - handle web IDE
   - Add tests

2. `src/routes/translator.rs`
   - `is_translatable_path()` - add new patterns

---

## Notes

- `github.dev` uses the same URL structure as `github.com`, just different domain
- GitLab Web IDE uses a completely different URL structure (`/-/ide/project/...`)
- Codespaces URLs include the repo in the path after `/codespaces/new/`
- Query params like `?ref_type=heads` and `?resume=1` can be safely ignored
