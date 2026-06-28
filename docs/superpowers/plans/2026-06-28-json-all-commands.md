# JSON for All Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `--json` produce JSON for every difftree command surface, while preserving the existing `difftree --pr --json` behavior.

**Architecture:** Keep the existing git-aware `ChangeTree` JSON path for comparison modes, including `--pr`. Add a plain-tree JSON collector in `src/view.rs` for classic tree output (`--plain`, non-git fallback, `-G/--git-status`, and `interactive --json`). Route JSON before terminal rendering whenever the invocation is a plain-tree command; explicit git comparison flags still require a git repository.

**Tech Stack:** Rust 2021, `clap` derive, `serde`/`serde_json`, `ignore::WalkBuilder`, existing `git2` status cache, `assert_cmd` CLI tests, `just all` final gate.

---

## Current Behavior Matrix

| Invocation | Current behavior | Target behavior |
| --- | --- | --- |
| `difftree --json` in a git repo | JSON `ChangeTree` | Unchanged |
| `difftree --pr --json` | JSON `ChangeTree` with `"Pr"` comparison | Unchanged and pinned |
| `difftree --all --json` in a git repo | JSON `ChangeTree` all-files view | Unchanged |
| `difftree --plain --json` | Terminal tree text | Plain-tree JSON |
| `difftree -G --json` | Terminal tree text with status column | Plain-tree JSON with `git_status` fields |
| `difftree --json <non-git-dir>` | Error: requires git repository | Plain-tree JSON plus the existing outside-git warning on stderr |
| `difftree --all --json <non-git-dir>` | Error: requires git repository | Plain-tree JSON plus the existing outside-git warning on stderr |
| `difftree --staged --json <non-git-dir>` | Error | Unchanged, because this is an explicit git comparison |
| `difftree interactive --json <path>` | Clap error | Plain-tree JSON export; do not start TUI |

## File Structure

| File | Responsibility | Change |
| --- | --- | --- |
| `tests/cli.rs` | CLI contract tests | Add failing tests for plain JSON, non-git JSON fallback, git-status JSON, interactive JSON, and `--pr --json` regression. Update the old outside-git JSON error test. |
| `src/view.rs` | Classic tree rendering | Add plain JSON structs, `run_json`, recursive tree collection, optional size/permissions/git status fields. Keep terminal `run` behavior intact. |
| `src/app.rs` | Clap definitions and arg conversion | Add `--json` to `InteractiveArgs`; add a conversion from `InteractiveArgs` to `ViewArgs` for JSON export. |
| `src/main.rs` | Command routing | Route plain-tree JSON before terminal rendering; keep explicit comparison modes on the existing `ChangeTree` path. |
| `README.md`, `docs/specs/difftree-decisions-v0.2.md` | User and contract docs | Document the plain-tree JSON mode and the `interactive --json` export behavior. |

---

## Task 1: Pin the command matrix with failing CLI tests

**Files:**
- Modify: `tests/cli.rs`

- [ ] **Step 1: Add plain JSON tests**

Append these tests near the existing JSON tests in `tests/cli.rs`:

```rust
#[test]
fn json_plain_flag_outputs_plain_tree_json() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    fs::create_dir(p.join("src"))?;
    fs::write(p.join("src/lib.rs"), "pub fn demo() {}\n")?;

    let output = Command::cargo_bin("difftree")?
        .arg("--plain")
        .arg("--json")
        .arg(p)
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let value: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(value["schema_version"], "difftree.v1");
    assert_eq!(value["view"], "plain-tree");
    assert_eq!(value["summary"]["directories"], 1);
    assert_eq!(value["summary"]["files"], 1);
    assert_eq!(value["root"]["children"][0]["name"], "src");
    Ok(())
}

#[test]
fn json_outside_git_repo_falls_back_to_plain_tree_json() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::write(temp_dir.path().join("a.txt"), "a\n")?;

    let output = Command::cargo_bin("difftree")?
        .arg("--json")
        .arg(temp_dir.path())
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    let value: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(value["view"], "plain-tree");
    assert_eq!(value["summary"]["files"], 1);
    assert!(stderr.contains("outside a git repository"));
    Ok(())
}

#[test]
fn all_json_outside_git_repo_falls_back_to_plain_tree_json() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::write(temp_dir.path().join("a.txt"), "a\n")?;

    let output = Command::cargo_bin("difftree")?
        .arg("--all")
        .arg("--json")
        .arg(temp_dir.path())
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    let value: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(value["view"], "plain-tree");
    assert_eq!(value["summary"]["files"], 1);
    assert!(stderr.contains("outside a git repository"));
    Ok(())
}
```

- [ ] **Step 2: Replace the old outside-git JSON error test**

Delete `test_json_outside_git_repo_errors`; the new `json_outside_git_repo_falls_back_to_plain_tree_json` test is the replacement. Keep `test_staged_outside_git_repo_errors` unchanged, because explicit comparison modes should still fail outside a repo.

- [ ] **Step 3: Add git-status JSON and interactive JSON tests**

Append:

```rust
#[test]
fn json_git_status_plain_tree_includes_status_fields() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("tracked.txt"), "one\n")?;
    Command::new("git").args(["add", "tracked.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    fs::write(p.join("tracked.txt"), "two\n")?;

    let output = Command::cargo_bin("difftree")?
        .arg("-G")
        .arg("--json")
        .arg(p)
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let value: serde_json::Value = serde_json::from_str(&stdout)?;
    let child = &value["root"]["children"][0];
    assert_eq!(child["name"], "tracked.txt");
    assert_eq!(child["git_status"], "Modified");
    Ok(())
}

#[test]
fn interactive_json_exports_plain_tree_without_tui() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::write(temp_dir.path().join("a.txt"), "a\n")?;

    let output = Command::cargo_bin("difftree")?
        .arg("interactive")
        .arg("--json")
        .arg(temp_dir.path())
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let value: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(value["view"], "plain-tree");
    assert_eq!(value["summary"]["files"], 1);
    Ok(())
}
```

- [ ] **Step 4: Strengthen the existing `--pr --json` regression**

Extend `pr_json_emits_pr_comparison` with a parse check so a future plain-JSON route cannot accidentally catch `--pr`:

```rust
        .stdout(predicate::str::contains("\"comparison\""))
        .stdout(predicate::str::contains("\"view\": \"blast-radius\""))
```

- [ ] **Step 5: Run the focused tests and confirm they fail for the expected reasons**

Run:

```bash
cargo test --test cli json_plain_flag_outputs_plain_tree_json
cargo test --test cli json_outside_git_repo_falls_back_to_plain_tree_json
cargo test --test cli all_json_outside_git_repo_falls_back_to_plain_tree_json
cargo test --test cli json_git_status_plain_tree_includes_status_fields
cargo test --test cli interactive_json_exports_plain_tree_without_tui
cargo test --test cli pr_json_emits_pr_comparison
```

Expected: the four new plain/interactive tests fail because JSON is not produced or clap rejects `interactive --json`; `pr_json_emits_pr_comparison` still passes.

---

## Task 2: Add plain-tree JSON collection in `src/view.rs`

**Files:**
- Modify: `src/view.rs`

- [ ] **Step 1: Add imports and serializable plain JSON types**

Update imports at the top of `src/view.rs`:

```rust
use serde::Serialize;
use std::path::Path;
```

Add these types after the imports:

```rust
#[derive(Debug, Serialize)]
struct PlainTreeJson {
    schema_version: &'static str,
    view: &'static str,
    root: PlainTreeNode,
    summary: PlainTreeSummary,
}

#[derive(Debug, Serialize)]
struct PlainTreeSummary {
    directories: usize,
    files: usize,
}

#[derive(Debug, Serialize)]
struct PlainTreeNode {
    name: String,
    path: String,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_status: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<String>,
    children: Vec<PlainTreeNode>,
}
```

- [ ] **Step 2: Add the JSON entry point**

Add below `pub fn run(...)`:

```rust
/// Executes the classic directory tree view as JSON.
pub fn run_json(args: &ViewArgs) -> anyhow::Result<()> {
    let tree = collect_plain_tree_json(args)?;
    println!("{}", serde_json::to_string_pretty(&tree)?);
    Ok(())
}
```

- [ ] **Step 3: Add the collector and recursive tree builder**

Add this helper block below `run_json`:

```rust
fn collect_plain_tree_json(args: &ViewArgs) -> anyhow::Result<PlainTreeJson> {
    if !args.path.is_dir() {
        anyhow::bail!("'{}' is not a directory.", args.path.display());
    }

    let canonical_root = fs::canonicalize(&args.path)?;
    let git_repo_status = if args.git_status { git::load_status(&canonical_root)? } else { None };
    let status_info = git_repo_status
        .as_ref()
        .map(|s| (&s.cache, s.root.as_path()));

    let mut builder = WalkBuilder::new(&args.path);
    builder.hidden(!args.show_all).git_ignore(args.gitignore);
    if let Some(level) = args.level {
        builder.max_depth(Some(level));
    }

    let mut entries: Vec<_> = builder
        .build()
        .filter_map(|result| match result {
            Ok(entry) if entry.depth() > 0 => Some(entry),
            Ok(_) => None,
            Err(err) => {
                eprintln!("difftree: ERROR: {err}");
                None
            }
        })
        .collect();

    let sort_options = args.to_sort_options();
    sort::sort_entries_hierarchically(&mut entries, &sort_options);

    let mut summary = PlainTreeSummary { directories: 0, files: 0 };
    let mut cursor = 0;
    let children = build_plain_json_children(
        &args.path,
        &entries,
        &mut cursor,
        &args.path,
        args,
        status_info,
        &mut summary,
    );

    Ok(PlainTreeJson {
        schema_version: "difftree.v1",
        view: "plain-tree",
        root: PlainTreeNode {
            name: args.path.display().to_string(),
            path: String::new(),
            kind: "Directory",
            git_status: None,
            size_bytes: None,
            permissions: root_permissions_json(args)?,
            children,
        },
        summary,
    })
}

fn build_plain_json_children(
    parent: &Path,
    entries: &[ignore::DirEntry],
    cursor: &mut usize,
    root: &Path,
    args: &ViewArgs,
    status_info: Option<(&git::StatusCache, &Path)>,
    summary: &mut PlainTreeSummary,
) -> Vec<PlainTreeNode> {
    let mut children = Vec::new();
    while *cursor < entries.len() {
        let entry = &entries[*cursor];
        if entry.path().parent() != Some(parent) {
            break;
        }

        let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
        *cursor += 1;

        if args.dirs_only && !is_dir {
            continue;
        }

        let mut node = plain_json_node(entry, root, args, status_info, is_dir);
        if is_dir {
            summary.directories += 1;
            node.children =
                build_plain_json_children(entry.path(), entries, cursor, root, args, status_info, summary);
        } else {
            summary.files += 1;
        }
        children.push(node);
    }
    children
}
```

- [ ] **Step 4: Add node metadata helpers**

Add below the recursive builder:

```rust
fn plain_json_node(
    entry: &ignore::DirEntry,
    root: &Path,
    args: &ViewArgs,
    status_info: Option<(&git::StatusCache, &Path)>,
    is_dir: bool,
) -> PlainTreeNode {
    let metadata = if args.size || args.permissions { entry.metadata().ok() } else { None };
    let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());

    PlainTreeNode {
        name: entry.file_name().to_string_lossy().to_string(),
        path: rel.to_string_lossy().to_string(),
        kind: if is_dir { "Directory" } else { "File" },
        git_status: git_status_json(entry.path(), status_info),
        size_bytes: if args.size && !is_dir { metadata.as_ref().map(|m| m.len()) } else { None },
        permissions: if args.permissions {
            metadata.as_ref().map(metadata_permissions_json).or_else(|| Some("----------".to_string()))
        } else {
            None
        },
        children: Vec::new(),
    }
}

fn root_permissions_json(args: &ViewArgs) -> anyhow::Result<Option<String>> {
    if !args.permissions {
        return Ok(None);
    }
    let md = fs::metadata(&args.path)?;
    Ok(Some(metadata_permissions_json(&md)))
}

fn metadata_permissions_json(md: &fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = md.permissions().mode();
        let file_type_char = if md.is_dir() { 'd' } else { '-' };
        format!("{}{}", file_type_char, utils::format_permissions(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = md;
        "----------".to_string()
    }
}

fn git_status_json(path: &Path, status_info: Option<(&git::StatusCache, &Path)>) -> Option<&'static str> {
    let (cache, repo_root) = status_info?;
    let canonical_entry = path.canonicalize().ok()?;
    let relative_path = canonical_entry.strip_prefix(repo_root).ok()?;
    cache.get(relative_path).map(file_status_json)
}

fn file_status_json(status: &git::FileStatus) -> &'static str {
    match status {
        git::FileStatus::Modified => "Modified",
        git::FileStatus::New => "New",
        git::FileStatus::Deleted => "Deleted",
        git::FileStatus::Renamed => "Renamed",
        git::FileStatus::Typechange => "Typechange",
        git::FileStatus::Untracked => "Untracked",
        git::FileStatus::Conflicted => "Conflicted",
    }
}
```

- [ ] **Step 5: Run focused compile/test**

Run:

```bash
cargo test --test cli json_plain_flag_outputs_plain_tree_json
cargo test --test cli json_git_status_plain_tree_includes_status_fields
```

Expected: these still fail until routing is added, but the crate compiles. If either fails to compile, fix the helper signatures before continuing.

---

## Task 3: Route top-level plain-tree JSON before terminal rendering

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Move `explicit_mode` computation before `wants_plain_tree`**

In `run_cli`, compute this before `wants_plain_tree`:

```rust
    let explicit_mode = view_args.uncommitted
        || view_args.unstaged
        || view_args.staged
        || view_args.range.is_some()
        || view_args.against.is_some()
        || view_args.pr.is_some();
```

Remove the later duplicate `let explicit_mode = ...` block.

- [ ] **Step 2: Add a plain JSON route**

Immediately after `explicit_mode`, add:

```rust
    let in_git_repo = is_git_repo(&view_args.path);
    let wants_plain_json = view_args.json
        && (view_args.plain || view_args.git_status || (!explicit_mode && !in_git_repo));
    if wants_plain_json {
        if !in_git_repo {
            eprintln!(
                "difftree: outside a git repository; showing plain tree (git features unavailable)"
            );
        }
        return view::run_json(view_args);
    }
```

Then update `wants_plain_tree` to reuse `in_git_repo`:

```rust
    let wants_plain_tree = view_args.plain
        || view_args.git_status
        || (!view_args.json
            && !view_args.all
            && !view_args.staged
            && !view_args.unstaged
            && !view_args.uncommitted
            && view_args.range.is_none()
            && view_args.against.is_none()
            && view_args.pr.is_none()
            && !view_args.ignored
            && !in_git_repo);
```

- [ ] **Step 3: Route `--all --json <non-git-dir>` after git collection returns `None`**

Inside `let Some(tree) = tree else { ... }`, before the `if view_args.json || explicit_mode` branch, add:

```rust
        if view_args.json && !explicit_mode {
            eprintln!(
                "difftree: outside a git repository; showing plain tree (git features unavailable)"
            );
            return view::run_json(view_args);
        }
```

This keeps `--staged --json <non-git-dir>` and `--pr --json <non-git-dir>` as hard errors, while making `--all --json <non-git-dir>` a plain JSON fallback.

- [ ] **Step 4: Run focused routing tests**

Run:

```bash
cargo test --test cli json_plain_flag_outputs_plain_tree_json
cargo test --test cli json_outside_git_repo_falls_back_to_plain_tree_json
cargo test --test cli all_json_outside_git_repo_falls_back_to_plain_tree_json
cargo test --test cli json_git_status_plain_tree_includes_status_fields
cargo test --test cli pr_json_emits_pr_comparison
cargo test --test cli test_staged_outside_git_repo_errors
```

Expected: all listed tests pass except `interactive_json_exports_plain_tree_without_tui`, which is handled in Task 4.

- [ ] **Step 5: Commit**

Run:

```bash
just lint
git add src/view.rs src/main.rs tests/cli.rs
git commit -m "feat: emit JSON for plain tree command paths"
```

---

## Task 4: Support `difftree interactive --json`

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Test: `tests/cli.rs`

- [ ] **Step 1: Add `--json` to `InteractiveArgs`**

In `src/app.rs`, add this field to `InteractiveArgs`:

```rust
    #[arg(long)]
    pub json: bool,
```

- [ ] **Step 2: Add conversion from interactive args to plain view args**

Add this method in the existing `impl InteractiveArgs` block:

```rust
    pub fn to_json_view_args(&self) -> ViewArgs {
        ViewArgs {
            path: self.path.clone(),
            json: true,
            show_all: self.all,
            gitignore: self.gitignore,
            git_status: self.git_status,
            icons: self.icons,
            size: self.size,
            permissions: self.permissions,
            level: self.expand_level,
            sort: self.sort,
            dirs_first: self.dirs_first,
            case_sensitive: self.case_sensitive,
            natural_sort: self.natural_sort,
            reverse: self.reverse,
            dotfiles_first: self.dotfiles_first,
            ..ViewArgs::default()
        }
    }
```

Note: `InteractiveArgs::all` maps to `ViewArgs::show_all`, not `ViewArgs::all`. In the interactive command, `-a/--all` is the classic "show hidden files" flag.

- [ ] **Step 3: Route interactive JSON in `main.rs`**

Update the command match:

```rust
    match &args.command {
        Some(Commands::Interactive(interactive_args)) if interactive_args.json => {
            let view_args = interactive_args.to_json_view_args();
            view::run_json(&view_args)
        }
        Some(Commands::Interactive(interactive_args)) => tui::run(interactive_args, &ls_colors),
        None => run_cli(&args, &ls_colors),
    }
```

- [ ] **Step 4: Run focused interactive tests**

Run:

```bash
cargo test --test cli interactive_json_exports_plain_tree_without_tui
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
just lint
git add src/app.rs src/main.rs tests/cli.rs
git commit -m "feat: add JSON export mode for interactive command"
```

---

## Task 5: Document the JSON contract and run the full gate

**Files:**
- Modify: `README.md`
- Modify: `docs/specs/difftree-decisions-v0.2.md`

- [ ] **Step 1: Update the decisions doc**

In `docs/specs/difftree-decisions-v0.2.md`, replace the `--json` flag table row with:

```markdown
| `--json` | Emit JSON. Git-aware comparison modes emit the shared `ChangeTree` model; plain/classic tree modes emit the plain-tree model. |
```

Under `## JSON schema`, append:

```markdown
Plain-tree JSON is used when no git comparison is selected (`--plain`, non-git fallback,
`-G/--git-status`, and `interactive --json`). It uses the same
`schema_version: "difftree.v1"` and contains:

- `view: "plain-tree"`.
- `root`: recursive tree nodes with `name`, `path`, `kind`, optional `git_status`,
  optional `size_bytes`, optional `permissions`, and `children`.
- `summary`: `directories` and `files` counts for entries included after filters.

Explicit git comparison modes (`--staged`, `--unstaged`, `--uncommitted`, `--range`,
`--against`, and `--pr`) still require a git repository when combined with `--json`.
```

- [ ] **Step 2: Update README usage**

In `README.md`, near the existing JSON mention, add:

```markdown
`--json` is available on every non-help command path. Git-aware comparison modes
(`difftree --json`, `--all --json`, `--pr --json`, and related comparison flags)
emit the `ChangeTree` model. Classic/plain tree paths (`--plain --json`, non-git
fallbacks, `-G --json`, and `interactive --json`) emit a plain-tree JSON model.
Explicit git comparisons such as `--staged --json` and `--pr --json` still require
a git repository.
```

- [ ] **Step 3: Run the focused command matrix**

Run:

```bash
cargo test --test cli json_plain_flag_outputs_plain_tree_json
cargo test --test cli json_outside_git_repo_falls_back_to_plain_tree_json
cargo test --test cli all_json_outside_git_repo_falls_back_to_plain_tree_json
cargo test --test cli json_git_status_plain_tree_includes_status_fields
cargo test --test cli interactive_json_exports_plain_tree_without_tui
cargo test --test cli pr_json_emits_pr_comparison
cargo test --test cli test_staged_outside_git_repo_errors
```

Expected: PASS.

- [ ] **Step 4: Run the full project gate**

Run:

```bash
just all
```

Expected: format, clippy, check, and tests all pass.

- [ ] **Step 5: Manual smoke checks**

Run:

```bash
cargo run -q -- --plain --json . | jq '.view'
cargo run -q -- -G --json . | jq '.root.children'
cargo run -q -- interactive --json . | jq '.view'
cargo run -q -- --pr --json | jq '.comparison'
```

Expected:

```text
"plain-tree"
```

The second command prints an array; the third prints `"plain-tree"`; the fourth prints a JSON object containing `"Pr"` when run in this worktree.

- [ ] **Step 6: Commit docs**

Run:

```bash
git add README.md docs/specs/difftree-decisions-v0.2.md
git commit -m "docs: document JSON support across command paths"
```

---

## Self-Review

- Spec coverage: The plan covers top-level git-aware JSON, `difftree --pr --json`, plain/classic JSON, outside-git fallback JSON, `-G --json`, `--all --json` outside git, explicit comparison errors outside git, and `interactive --json`.
- Placeholder scan: No placeholder steps remain; each task has concrete files, code snippets, commands, and expected outcomes.
- Type consistency: `InteractiveArgs::to_json_view_args` maps interactive `--all` to `ViewArgs::show_all`; this avoids colliding with the git-aware `ViewArgs::all` view flag.
