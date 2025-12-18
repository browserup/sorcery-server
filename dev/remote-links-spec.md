# Srcuri Translator Mode – Server-Side Specification

**File:** `srcuri-translator-mode-spec.md`  
**Audience:** Srcuri server implementers (LLM or human)  
**Scope:** How `https://srcuri.com` behaves in **translator mode**, converting web repo URLs (GitHub, GitLab, Bitbucket, Gitea, Codeberg, Azure DevOps, etc.) into canonical `srcuri://…` targets and redirecting to their HTTPS “mirror” form for use in Slack, Teams, Jira, etc.

---

## 1. Concepts & Goals

### 1.1 Core Concepts

Srcuri has two logical URL “modes” on `https://srcuri.com`:

1. **Mirror Mode (existing behavior)**  
   A direct **web mirror** of the custom protocol `srcuri://…` links, used so tools like Slack/Teams (which don’t allow custom protocols) can click an HTTPS link.

   - Custom protocol example:

     ```text
     srcuri://browserup/web/app/models/metric_set.rb:42?branch=f41ccd0b6a4ec5d964ac26377c66e34973e3581a&remote=github.com/browserup/browserup
     ```

   - Web mirror:

     ```text
     https://srcuri.com/browserup/web/app/models/metric_set.rb:42?branch=f41ccd0b6a4ec5d964ac26377c66e34973e3581a&remote=github.com/browserup/browserup
     ```

   Semantics: “This **is** a srcuri link, just over HTTPS.”

2. **Translator Mode (new behavior)**  
   A **viral on-ramp** that accepts a *web repo URL* (e.g. GitHub file URL) and converts it into a Srcuri target, then redirects to the canonical mirror-mode URL.

   Example user entry:

   ```text
   https://srcuri.com/?remote=https://github.com/browserup/browserup/blob/f41ccd0b6a4ec5d964ac26377c66e34973e3581a/web/app/models/metric_set.rb#L42
   ```

   The server parses that and redirects to:

   ```text
   https://srcuri.com/browserup/web/app/models/metric_set.rb:42?branch=f41ccd0b6a4ec5d964ac26377c66e34973e3581a&remote=github.com/browserup/browserup
   ```

   From there, the **desktop app** will handle `srcuri://…` semantics (branch vs tag vs SHA, worktrees, etc.).

### 1.2 Goals

Translator mode is designed to:

1. **Minimize friction (virality)**  
   - The smallest possible change to a GitHub/GitLab/etc URL to “make it Srcuri-aware.”  
   - Ideal mental model:  
     > “Take a repo link and feed it to srcuri; srcuri figures the rest out.”

2. **Require minimal protocol knowledge**  
   - Typical devs should not need to understand `branch`, `remote`, or any query-parameter semantics.  
   - They just know: *“I paste my GitHub URL into srcuri.com in the right place; it opens in my editor.”*

3. **Keep the protocol surface stable**  
   - Translator mode **produces** canonical `srcuri://…` shapes that the desktop app can rely on.  
   - It does *not* attempt to do git operations; it only manipulates strings.

---

## 2. High-Level Behavior

### 2.1 Mode Detection

Given an HTTP request to `https://srcuri.com`:

- **Translator Mode (this spec):**
  - Path is empty or `/` (i.e. root), **and**
  - Query contains `remote=`

  ```text
  GET /?remote=https://github.com/... HTTP/1.1
  Host: srcuri.com
  ```

- **Mirror Mode (handled elsewhere):**
  - Path is **non-empty**, e.g.

    ```text
    GET /browserup/web/app/models/metric_set.rb:42?branch=main&remote=github.com/browserup/browserup
    ```

  - In mirror mode, the server simply renders a landing page / OpenGraph + triggers `srcuri://…` if appropriate.

**Rule:**

> If `path != "/"` (non-empty path), treat as **mirror mode** and ignore `remote` as a translator hint.  
> If `path == "/"` and `remote` is present, treat as **translator mode**.

### 2.2 Translator Mode Flow

1. Extract the `remote` query parameter:
   ```text
   remote = URLSearchParams(request.query).get("remote")
   ```

2. Parse this `remote` value as a **web repo URL** (GitHub, GitLab, etc.).

3. Convert that into a canonical internal object `SrcuriTarget`:

   ```text
   SrcuriTarget {
     remote:    String,  // normalized repo identity: "github.com/owner/repo"
     repo_name: String,  // "repo"
     ref:       String,  // branch/tag/SHA string (generic "ref")
     file_path: String,  // "path/inside/repo.ext"
     line:      u32?     // optional, from fragment or query
   }
   ```

4. Render `SrcuriTarget` into a **canonical mirror-mode URL**:

   ```text
   https://srcuri.com/<repo_name>/<file_path>[:<line>]?
       branch=<ref>&remote=<remote>
   ```

   - For now, translator **always** uses `branch=<ref>` even if it’s actually a tag or commit; the desktop app may later refine interpretation based on git metadata.
   - `remote` is the normalized repo identity, *not* necessarily the full original URL.

5. HTTP 302 redirect to that canonical URL.

6. Mirror mode then handles page rendering + optional `srcuri://…` activation.

---

## 3. Common Output Format

Regardless of provider (GitHub, GitLab, Bitbucket, Gitea, Codeberg, Azure DevOps), translator mode produces a `SrcuriTarget` and then:

```text
srcuri://<repo_name>/<file_path>[:<line>]?
    branch=<ref>&remote=<remote>
```

Mirrored as HTTPS:

```text
https://srcuri.com/<repo_name>/<file_path>[:<line>]?
    branch=<ref>&remote=<remote>
```

Where:

- `remote` – normalized repo identity for local remote matching:
  - `github.com/owner/repo`
  - `gitlab.com/group/project`
  - `bitbucket.org/workspace/repo`
  - `codeberg.org/user/repo`
  - `gitea.com/org/repo`
  - `dev.azure.com/org/project/_git/repo`
- `branch` – generic Git ref (branch, tag, or commit SHA). The desktop app will later disambiguate; for translator mode it’s a simple string.

---

## 4. Fragment → Line Extraction

Translator mode must handle line numbers encoded in fragments or query parameters.

### 4.1 GitHub / GitLab / Gitea / Codeberg

- Fragment forms:
  - `#L10` → line = `10`
  - `#L10-L12` → take the first: line = `10`

Algorithm:

1. Remove leading `#`.
2. If starts with `"L"`:
   - Strip `"L"`, split on `-`, parse first number as `u32`.
3. Else: `line = None`.

### 4.2 Bitbucket Cloud

- Fragment forms:
  - `#lines-5` → line = `5`
  - `#lines-5:10` → line = `5`

Algorithm:

1. Remove `#`.
2. If starts with `"lines-"`:
   - Strip `"lines-"`, split on `:` or `-`, parse first number.

### 4.3 Azure DevOps

- Line is **not** in fragment; use `line` query parameter:
  - `?line=12` ⇒ line = `12`

If parsing fails or line is missing, `line = None` and the file opens without a specific line hint.

---

## 5. Provider-Specific Translation Rules

### 5.1 GitHub

**Supported pattern (file view):**

```text
https://github.com/:owner/:repo/(blob|blame)/:ref/:path[#L<line>[-L<end>]]
```

- `owner` – GitHub user or org
- `repo` – repository name
- `blob|blame` – view type (both supported)
- `ref` – branch, tag, or commit SHA (translator treats it generically)
- `path` – file path inside repo

#### Extraction

For a parsed `URL` with hostname `github.com`:

- Path segments (non-empty):
  - `segments[0] = owner`
  - `segments[1] = repo`
  - `segments[2] = "blob" | "blame"`
  - `segments[3] = ref`
  - `segments[4..] = file_path segments`
- Fragment → line (GitHub-style `#L42`)
- `remote = "github.com/owner/repo"`
- `repo_name = repo`
- `ref = segments[3]`
- `file_path = join(segments[4..], "/")`

#### Example 1 – Branch/commit SHA

Input translator URL:

```text
https://srcuri.com/?remote=https://github.com/browserup/browserup/blob/f41ccd0b6a4ec5d964ac26377c66e34973e3581a/web/app/models/metric_set.rb#L42
```

Parsed:

- `owner`     = `browserup`
- `repo`      = `browserup`
- `ref`       = `f41ccd0b6a4ec5d964ac26377c66e34973e3581a`
- `file_path` = `web/app/models/metric_set.rb`
- `line`      = `42`

`SrcuriTarget`:

```text
remote    = "github.com/browserup/browserup"
repo_name = "browserup"
ref       = "f41ccd0b6a4ec5d964ac26377c66e34973e3581a"
file_path = "web/app/models/metric_set.rb"
line      = 42
```

Redirect URL (mirror-mode target):

```text
https://srcuri.com/browserup/web/app/models/metric_set.rb:42?
    branch=f41ccd0b6a4ec5d964ac26377c66e34973e3581a&
    remote=github.com/browserup/browserup
```

### 5.2 GitLab.com

**Supported pattern (file view):**

```text
https://gitlab.com/:group/:project/-/(blob|blame|raw)/:ref/:path[#L<line>[-L<end>]]
```

- Modern GitLab always uses `/-/` before `blob`, `blame`, or `raw` for file URLs.

#### Extraction

For hostname `gitlab.com`:

- Path segments:
  - `segments[0] = group`
  - `segments[1] = project`
  - `segments[2] = "-"`
  - `segments[3] = blob | blame | raw`
  - `segments[4] = ref`
  - `segments[5..] = file_path`
- Fragment → line (`#L42`)
- `remote    = "gitlab.com/group/project"`
- `repo_name = project`
- `ref       = segments[4]`
- `file_path = join(segments[5..], "/")`

#### Example – Master branch

Input:

```text
https://srcuri.com/?remote=https://gitlab.com/gitlab-org/gitlab/-/blob/master/lib/gitlab/ci/templates/OpenShift.gitlab-ci.yml#L12
```

Parsed:

- `group`     = `gitlab-org`
- `project`   = `gitlab`
- `ref`       = `master`
- `file_path` = `lib/gitlab/ci/templates/OpenShift.gitlab-ci.yml`
- `line`      = `12`

Redirect URL:

```text
https://srcuri.com/gitlab/lib/gitlab/ci/templates/OpenShift.gitlab-ci.yml:12?
    branch=master&
    remote=gitlab.com/gitlab-org/gitlab
```

### 5.3 Bitbucket Cloud

**Supported pattern (file view):**

```text
https://bitbucket.org/:workspace/:repo/src/:ref/:path[#lines-<line>[:<end>]]
```

#### Extraction

For hostname `bitbucket.org`:

- Path segments:
  - `segments[0] = workspace`
  - `segments[1] = repo`
  - `segments[2] = "src"`
  - `segments[3] = ref`
  - `segments[4..] = file_path`
- Fragment → line (Bitbucket-style `#lines-5` or `#lines-5:10`)
- `remote    = "bitbucket.org/workspace/repo"`
- `repo_name = repo`
- `ref       = segments[3]`
- `file_path = join(segments[4..], "/")`

#### Example – Branch `master`

Input:

```text
https://srcuri.com/?remote=https://bitbucket.org/tutorials/markdowndemo/src/master/README.md#lines-5
```

Parsed:

- `workspace` = `tutorials`
- `repo`      = `markdowndemo`
- `ref`       = `master`
- `file_path` = `README.md`
- `line`      = `5`

Redirect URL:

```text
https://srcuri.com/markdowndemo/README.md:5?
    branch=master&
    remote=bitbucket.org/tutorials/markdowndemo
```

### 5.4 Gitea

**Supported patterns (file view):**

```text
https://gitea.com/:owner/:repo/src/branch/:ref/:path[#L<line>]
https://gitea.com/:owner/:repo/src/tag/:ref/:path[#L<line>]
https://gitea.com/:owner/:repo/src/commit/:ref/:path[#L<line>]
```

For translator mode V1, we treat all of these generically as `ref` but preserve the string exactly.

#### Extraction

- Path segments:
  - `segments[0] = owner`
  - `segments[1] = repo`
  - `segments[2] = "src"`
  - `segments[3] = "branch" | "tag" | "commit"` (ignored by translator)
  - `segments[4] = ref`
  - `segments[5..] = file_path`
- Fragment → line (`#L42`)
- `remote    = "gitea.com/owner/repo"`
- `repo_name = repo`
- `ref       = segments[4]`
- `file_path = join(segments[5..], "/")`

#### Example – Branch `main`

Input:

```text
https://srcuri.com/?remote=https://gitea.com/gitea/tea/src/branch/main/cmd/login.go#L24
```

Parsed:

- `owner`     = `gitea`
- `repo`      = `tea`
- `ref`       = `main`
- `file_path` = `cmd/login.go`
- `line`      = `24`

Redirect URL:

```text
https://srcuri.com/tea/cmd/login.go:24?
    branch=main&
    remote=gitea.com/gitea/tea
```

### 5.5 Codeberg (Forgejo)

Codeberg runs Forgejo and uses the same patterns as Gitea for file views:

```text
https://codeberg.org/:owner/:repo/src/branch/:ref/:path[#L<line>]
https://codeberg.org/:owner/:repo/src/tag/:ref/:path[#L<line>]
https://codeberg.org/:owner/:repo/src/commit/:ref/:path[#L<line>]
```

#### Extraction

Identical to Gitea, but `hostname == "codeberg.org"`.

Example:

```text
https://srcuri.com/?remote=https://codeberg.org/user/repo/src/branch/main/path/to/file.go#L10
```

Redirect URL:

```text
https://srcuri.com/repo/path/to/file.go:10?
    branch=main&
    remote=codeberg.org/user/repo
```

### 5.6 Azure DevOps

Azure DevOps file view URLs use query parameters rather than path segments.

**Supported pattern (canonical form):**

```text
https://dev.azure.com/:org/:project/_git/:repo?path=/:path&version=GB<ref>&line=<line>
```

Or shorter form (no project segment):

```text
https://dev.azure.com/:org/_git/:repo?path=/:path&version=GB<ref>&line=<line>
```

#### Extraction

For hostname `dev.azure.com`:

1. Path segments:

   - **Long form:**
     - `segments[0] = org`
     - `segments[1] = project`
     - `segments[2] = "_git"`
     - `segments[3] = repo`

     `remote = "dev.azure.com/org/project/_git/repo"`

   - **Short form:**
     - `segments[0] = org`
     - `segments[1] = "_git"`
     - `segments[2] = repo`

     `remote = "dev.azure.com/org/_git/repo"`

2. Query parameters:

   - `path` – leading `/` removed → file_path
   - `version` – e.g. `GBmain`, `GTv1.0.0`, `GCdeadbeef`
     - Translator strips the first two characters (`GB`, `GT`, `GC`) and uses the remainder as `ref`.
   - `line` – if present, parse as `u32` → line

3. `repo_name = repo`

#### Example – Branch `main`

Input:

```text
https://srcuri.com/?remote=https://dev.azure.com/fabric/fabric-editor/_git/fabric?path=/src/index.ts&version=GBmain&line=12
```

Parsed:

- `remote    = "dev.azure.com/fabric/fabric-editor/_git/fabric"`
- `repo_name = "fabric"`
- `file_path = "src/index.ts"`
- `ref       = "main"`  (strip `GB`)
- `line      = 12`

Redirect URL:

```text
https://srcuri.com/fabric/src/index.ts:12?
    branch=main&
    remote=dev.azure.com/fabric/fabric-editor/_git/fabric
```

---

## 6. Handling `remote` + Other Query Parameters

### 6.1 On `https://srcuri.com` (translator mode URL)

If translator mode is active (root path + `remote=` present):

- If `remote` contains a full **file URL** with a recognizable provider pattern (e.g. includes `/blob/`, `/src/`, `path=`, etc.):
  - The translator **derives everything** (ref, file_path, line) from that `remote` URL.
  - Any additional query params on `srcuri.com` such as `rev=`, `branch=`, `tag=`, `path=`, `line=` are **ignored**.
  - Rationale: the canonical, user-visible source of truth is the actual repo URL.

- If `remote` points only to a **repo root** (no file path, no provider “magic token”):
  - (Optional, advanced) you MAY support extra query params on `srcuri.com` such as `path=`, `rev=`, `branch=`, `tag=`, `line=` to build a `SrcuriTarget`.  
  - This is **not required** for translator mode V1; the primary viral path is full file URLs.

### 6.2 In the generated `srcuri://` and mirror URLs

- Translator always emits:
  - `remote=<normalized-repo-identity>`
  - `branch=<ref>`

- It never emits multiple git reference params (e.g. `branch` + `tag`).  
  There is **exactly one** git ref parameter.

The desktop app may later add support for more nuanced parameters (e.g. `commit=`, `tag=`) based on local git inspection, but that is outside this spec.

---

## 7. Error Handling

Translator mode should handle invalid/missing data gracefully:

1. **Missing `remote`**  
   - If `path == "/"` and `remote` is missing or empty → **not translator mode**.
   - Treat as standard homepage / landing page.

2. **Unsupported or unknown provider**  
   - If `remote` URL hostname is not recognized (not one of the known providers), or
   - The path structure doesn’t match any supported patterns:
     - Option A: Show an error page explaining that this URL format is not yet supported.
     - Option B: Show a simple “We can’t parse this repo link yet” with a button to “Open original URL”.

3. **Malformed `remote` URL**  
   - If `remote` is not a valid URL:
     - Treat as error, show message to user (e.g. “remote must be a valid URL”).

4. **Missing file path**  
   - If provider matches, but there is no file path segment (e.g. tree view or repo root):
     - Translator may either:
       - Decline to translate (error), or
       - Generate a srcuri URL without a specific file (e.g. `srcuri://repo/`).
     - For V1, simplest is to **only support file URLs** and show an error for others.

5. **Line parse failures**  
   - If fragment/query line parsing fails, simply set `line = None` and proceed.

---

## 8. Summary of Example Resolutions

Below is a compact table of example translator inputs and their resolved canonical mirror URLs.

| Provider   | Translator Input (`remote=`)                                                                                                                  | Resolved Mirror URL                                                                                                                                                |
|-----------|-----------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| GitHub    | `https://github.com/ericbeland/ruby-packer/blob/ffi-update/Gemfile#L10`                                                                      | `https://srcuri.com/ruby-packer/Gemfile:10?branch=ffi-update&remote=github.com/ericbeland/ruby-packer`                                                            |
| GitHub    | `https://github.com/browserup/browserup/blob/f41ccd0b6a4ec5d964ac26377c66e34973e3581a/web/app/models/metric_set.rb#L42`                       | `https://srcuri.com/browserup/web/app/models/metric_set.rb:42?branch=f41ccd0b6a4ec5d964ac26377c66e34973e3581a&remote=github.com/browserup/browserup`              |
| GitLab    | `https://gitlab.com/gitlab-org/gitlab/-/blob/master/lib/gitlab/ci/templates/OpenShift.gitlab-ci.yml#L12`                                     | `https://srcuri.com/gitlab/lib/gitlab/ci/templates/OpenShift.gitlab-ci.yml:12?branch=master&remote=gitlab.com/gitlab-org/gitlab`                                  |
| Bitbucket | `https://bitbucket.org/tutorials/markdowndemo/src/master/README.md#lines-5`                                                                  | `https://srcuri.com/markdowndemo/README.md:5?branch=master&remote=bitbucket.org/tutorials/markdowndemo`                                                            |
| Gitea     | `https://gitea.com/gitea/tea/src/branch/main/cmd/login.go#L24`                                                                                | `https://srcuri.com/tea/cmd/login.go:24?branch=main&remote=gitea.com/gitea/tea`                                                                                    |
| Codeberg  | `https://codeberg.org/user/repo/src/branch/main/path/to/file.go#L10`                                                                         | `https://srcuri.com/repo/path/to/file.go:10?branch=main&remote=codeberg.org/user/repo`                                                                             |
| Azure     | `https://dev.azure.com/fabric/fabric-editor/_git/fabric?path=/src/index.ts&version=GBmain&line=12`                                           | `https://srcuri.com/fabric/src/index.ts:12?branch=main&remote=dev.azure.com/fabric/fabric-editor/_git/fabric`                                                     |

These examples cover the core translation behavior for translator mode in the server.

---

## 9. Implementation Notes

- The spec is **language agnostic**, but implementations will typically:
  - Parse the outer `srcuri.com` URL.
  - Extract and parse the inner `remote` URL using a standard URL library.
  - Use host-based dispatch (GitHub/GitLab/Bitbucket/Gitea/Codeberg/Azure) to extract fields.
  - Construct `SrcuriTarget`, then render the canonical mirror URL and 302 redirect.

- Desktop and CLI tools should treat the generated mirror URLs as authoritative and not rely on the original `remote` file URL once translated.

