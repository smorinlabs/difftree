# All-files View + Comparison-Mode Flag Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separate *view* (`blast-radius` vs `all-files`) from *comparison* (`staged`/`unstaged`/`uncommitted`/…), rename the combined mode flag from `--all` to `--uncommitted`, add an explicit `--staged`, and make `--all`/`--tree` render a real all-files tree with git change marks overlaid.

**Architecture:** One core engine in `src/lib.rs` builds a serializable `ChangeTree` consumed by both `TerminalRenderer` and `JsonRenderer`. Blast-radius collection stays as-is; a new `collect_all_files` walks the filesystem with the `ignore` crate and overlays the git change map onto every file. `src/main.rs` resolves view and comparison independently and dispatches.

**Tech Stack:** Rust 2021, `git2`, `ignore`, `clap` (derive), `serde`/`serde_json`, `assert_cmd` + `predicates` + `tempfile` for CLI integration tests.

## Global Constraints

- Edition `2021`; crate + binary name `difftree`.
- `just all` (rustfmt + `cargo clippy -- -D warnings` + `cargo check` + `cargo test`) MUST pass at the end of every task. Clippy warnings are hard errors.
- JSON `schema_version` stays the literal `"difftree.v1"` (v1 unreleased; no compat guarantees broken).
- Existing 22 tests must stay green except where a task explicitly rewrites a test's expectation.
- TDD: write the failing test first, see it fail, implement minimally, see it pass, commit.
- Work on branch `codex/implement-prd-from-difftree-goal-v0.2`.

---

## File Structure

| File | Responsibility | This plan |
|---|---|---|
| `src/lib.rs` | Core model, git collection, renderers | Rename `All`→`Uncommitted`; add `View` enum + `ChangeTree.view`; refactor `build_tree` to take prepared maps + view; rollup refinement; new `collect_all_files` + `WalkOpts` |
| `src/app.rs` | clap flag surface | Add `--uncommitted`, `--staged`/`--cached`; repurpose `--all` (alias `--tree`) into a view bool |
| `src/main.rs` | Resolve view + comparison, dispatch | New mode/view resolution; dispatch all-files to `collect_all_files`; generalize fallback wording |
| `tests/cli.rs` | CLI integration tests | New tests for each behavior |
| `docs/specs/difftree-decisions-v0.2.md`, `docs/PRD/difftree-prd-v0.2.md`, `CHANGELOG.md` | Contracts/docs | Record the `--all`/`--uncommitted` inversion |

---

## Task 1: Rename internal comparison mode `All` → `Uncommitted`

Pure rename — no flag or behavior change yet. `--all` continues to select this mode via `main.rs` until Task 2.

**Files:**
- Modify: `src/lib.rs` (enum `ComparisonMode`, `diff_files`)
- Modify: `src/main.rs` (mode selection arm)

**Interfaces:**
- Produces: `ComparisonMode::Uncommitted` (replaces `ComparisonMode::All`), same payload semantics (HEAD vs working-tree+index).

- [ ] **Step 1: Rename the enum variant**

In `src/lib.rs`, change the `ComparisonMode` definition:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComparisonMode {
    Staged,
    Unstaged,
    Uncommitted,
    Range { range: String },
    Against { reference: String },
}
```

- [ ] **Step 2: Update the `diff_files` match arm**

In `src/lib.rs::diff_files`, rename the arm (body unchanged):

```rust
        ComparisonMode::Uncommitted => repo.diff_tree_to_workdir_with_index(
            repo.head().ok().and_then(|h| h.peel_to_tree().ok()).as_ref(),
            Some(&mut opts),
        )?,
```

- [ ] **Step 3: Update the caller in `main.rs`**

In `src/main.rs::run_cli`, in the `mode` selection, rename the arm:

```rust
    } else if view_args.all {
        ComparisonMode::Uncommitted
    } else if view_args.unstaged {
```

- [ ] **Step 4: Build and run the full suite**

Run: `cargo test`
Expected: PASS — `test result: ok. 22 passed` (no test references the old variant name).

- [ ] **Step 5: Lint + format gate**

Run: `just all`
Expected: all stages succeed, no clippy warnings.

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/main.rs
git commit -m "refactor: rename ComparisonMode::All to Uncommitted"
```

---

## Task 2: Add `--uncommitted` and `--staged`/`--cached` flags; fallback only on bare default

`--unstaged` keeps its meaning. `--uncommitted` becomes the explicit name for the combined mode. `--staged` (alias `--cached`) explicitly selects staged and does NOT auto-fallback; bare `difftree` keeps the staged→unstaged fallback.

**Files:**
- Modify: `src/app.rs` (`ViewArgs`)
- Modify: `src/main.rs` (`run_cli` mode + fallback resolution)
- Test: `tests/cli.rs`

**Interfaces:**
- Consumes: `ComparisonMode::Uncommitted` (Task 1).
- Produces: `ViewArgs.uncommitted: bool`, `ViewArgs.staged: bool`. `--all` field still exists but is no longer wired to a comparison (it becomes a view in Task 6).

- [ ] **Step 1: Write the failing test for `--uncommitted`**

Append to `tests/cli.rs`:

```rust
#[test]
fn test_uncommitted_shows_staged_and_unstaged() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("base.txt"), "one")?;
    Command::new("git").args(["add", "base.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    // one staged new file + one unstaged modification
    fs::write(p.join("staged.txt"), "s")?;
    Command::new("git").args(["add", "staged.txt"]).current_dir(p).output()?;
    fs::write(p.join("base.txt"), "two")?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--uncommitted").arg(p);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("staged.txt"))
        .stdout(predicate::str::contains("base.txt"));
    Ok(())
}
```

- [ ] **Step 2: Write the failing test for explicit `--staged` (no fallback)**

Append to `tests/cli.rs`:

```rust
#[test]
fn test_staged_flag_does_not_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("base.txt"), "one")?;
    Command::new("git").args(["add", "base.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    fs::write(p.join("base.txt"), "two")?; // only unstaged

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--staged").arg(p);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No staged changes").not());
    Ok(())
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --test cli test_uncommitted_shows_staged_and_unstaged test_staged_flag_does_not_fallback`
Expected: FAIL — `--uncommitted`/`--staged` are unknown clap arguments (process exits non-zero with "unexpected argument").

- [ ] **Step 4: Add the flags in `app.rs`**

Today `ViewArgs` has `pub tree: bool` and `pub all: bool` (consecutive), and `pub unstaged: bool` further down. Add `staged` (with `cached` alias) and `uncommitted` next to `unstaged`. In `src/app.rs`, find the existing block:

```rust
    #[arg(long)]
    pub unstaged: bool,
    #[arg(long)]
    pub all: bool,
```

and replace it with:

```rust
    #[arg(long, alias = "cached")]
    pub staged: bool,
    #[arg(long)]
    pub unstaged: bool,
    #[arg(long)]
    pub uncommitted: bool,
    #[arg(long)]
    pub all: bool,
```

Leave the existing `pub tree: bool` field as-is for now (Task 6 folds it into `--all` as an alias). Do not add any `-a` short form to these flags — `-a` stays bound to `show_all`.

- [ ] **Step 5: Wire comparison + fallback in `main.rs`**

In `src/main.rs::run_cli`, replace the `mode`/`tree` resolution block (everything from `let mode = if let Some(range)` through the `let Some(tree) = tree else {...};`) with:

```rust
    let mode = if let Some(range) = &view_args.range {
        ComparisonMode::Range { range: range.clone() }
    } else if let Some(reference) = &view_args.against {
        ComparisonMode::Against { reference: reference.clone() }
    } else if view_args.uncommitted {
        ComparisonMode::Uncommitted
    } else if view_args.unstaged {
        ComparisonMode::Unstaged
    } else {
        ComparisonMode::Staged
    };
    let explicit_mode = view_args.uncommitted
        || view_args.unstaged
        || view_args.staged
        || view_args.range.is_some()
        || view_args.against.is_some();
    let use_fallback = !explicit_mode && !view_args.tree && !view_args.all && !view_args.ignored;
    let tree = if use_fallback {
        collect_default_with_fallback(&view_args.path)?
    } else {
        collect_changes(&view_args.path, mode, true)?
    };
    let Some(tree) = tree else {
        return view::run(view_args, ls_colors);
    };
```

- [ ] **Step 6: Run the new tests to verify they pass**

Run: `cargo test --test cli test_uncommitted_shows_staged_and_unstaged test_staged_flag_does_not_fallback`
Expected: PASS (2 passed).

- [ ] **Step 7: Full gate**

Run: `just all`
Expected: all green, no clippy warnings.

- [ ] **Step 8: Commit**

```bash
git add src/app.rs src/main.rs tests/cli.rs
git commit -m "feat: add --uncommitted and --staged comparison flags"
```

---

## Task 3: Add `View` enum and `view` field to the JSON model

**Files:**
- Modify: `src/lib.rs` (`View` enum, `ChangeTree`, `build_tree` signature + construction, both collectors)
- Test: `tests/cli.rs`

**Interfaces:**
- Produces: `pub enum View { BlastRadius, AllFiles }` serializing to `"blast-radius"`/`"all-files"`; `ChangeTree.view: View`; `build_tree(root_name, mode, view, files, fallback)`.

- [ ] **Step 1: Write the failing test**

Append to `tests/cli.rs`:

```rust
#[test]
fn test_json_includes_view_field() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("changed.txt"), "hi")?;
    Command::new("git").args(["add", "changed.txt"]).current_dir(p).output()?;

    let output = Command::cargo_bin("difftree")?.arg("--json").arg(p).output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("\"view\": \"blast-radius\""));
    Ok(())
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --test cli test_json_includes_view_field`
Expected: FAIL — output has no `"view"` field.

- [ ] **Step 3: Add the `View` enum**

In `src/lib.rs`, after the `ComparisonMode` definition, add:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum View {
    BlastRadius,
    AllFiles,
}
```

- [ ] **Step 4: Add the field to `ChangeTree`**

In `src/lib.rs`, add `view` to the struct (place it after `comparison`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeTree {
    pub schema_version: String,
    pub comparison: ComparisonMode,
    pub view: View,
    pub root: TreeNode,
    pub summary: Rollup,
    pub fallback: Option<String>,
}
```

- [ ] **Step 5: Thread `view` through `build_tree`**

In `src/lib.rs::build_tree`, change the signature to add a `view: View` parameter after `mode`:

```rust
fn build_tree(
    root_name: String,
    mode: ComparisonMode,
    view: View,
    files: Vec<FileChange>,
    fallback: Option<String>,
) -> ChangeTree {
```

And in its final `ChangeTree { ... }` construction, add `view,`:

```rust
    ChangeTree { schema_version: SCHEMA_VERSION.into(), comparison: mode, view, root, summary, fallback }
```

- [ ] **Step 6: Update the two `build_tree` call sites**

In `src/lib.rs::collect_changes`, pass `View::BlastRadius`:

```rust
    Ok(Some(build_tree(root_name, mode, View::BlastRadius, files, None)))
```

(`collect_default_with_fallback` calls `collect_changes`, so no change needed there.)

- [ ] **Step 7: Run the test to verify it passes**

Run: `cargo test --test cli test_json_includes_view_field`
Expected: PASS.

- [ ] **Step 8: Full gate**

Run: `just all`
Expected: green (the existing `test_json_schema_version_for_staged_change` still passes — the new field is additive).

- [ ] **Step 9: Commit**

```bash
git add src/lib.rs tests/cli.rs
git commit -m "feat: add view field (blast-radius|all-files) to JSON model"
```

---

## Task 4: Refactor `build_tree` to take prepared maps; refine rollups for unchanged files

Prepares `build_tree` to accept a full directory+file set (needed by the all-files walk) and makes rollups count only *changed* files/dirs even when unchanged (`Clean`) nodes are present. Blast-radius behavior is unchanged because its trees contain no `Clean` files.

**Files:**
- Modify: `src/lib.rs` (`build_tree`, `collect_changes`, new helper `files_to_maps`)

**Interfaces:**
- Produces: `build_tree(root_name, mode, view, dirs: BTreeSet<PathBuf>, fmap: BTreeMap<PathBuf, FileChange>, fallback)` and `files_to_maps(files) -> (BTreeSet<PathBuf>, BTreeMap<PathBuf, FileChange>)`.

- [ ] **Step 1: Add the `files_to_maps` helper**

In `src/lib.rs`, add above `build_tree`:

```rust
fn files_to_maps(
    files: Vec<FileChange>,
) -> (BTreeSet<PathBuf>, BTreeMap<PathBuf, FileChange>) {
    let mut dirset = BTreeSet::new();
    let mut fmap = BTreeMap::new();
    for f in files {
        for a in f.path.ancestors().skip(1) {
            if !a.as_os_str().is_empty() {
                dirset.insert(a.to_path_buf());
            }
        }
        fmap.insert(f.path.clone(), f);
    }
    (dirset, fmap)
}
```

- [ ] **Step 2: Change `build_tree` to take prepared maps**

In `src/lib.rs::build_tree`, replace the signature and delete the now-duplicated map-building loop at the top of the body:

```rust
fn build_tree(
    root_name: String,
    mode: ComparisonMode,
    view: View,
    dirset: BTreeSet<PathBuf>,
    fmap: BTreeMap<PathBuf, FileChange>,
    fallback: Option<String>,
) -> ChangeTree {
```

Remove these lines (the old in-body construction) from the start of the body:

```rust
    let mut dirset = BTreeSet::new();
    let mut fmap = BTreeMap::new();
    for f in files {
        for a in f.path.ancestors().skip(1) {
            if !a.as_os_str().is_empty() {
                dirset.insert(a.to_path_buf());
            }
        }
        fmap.insert(f.path.clone(), f);
    }
```

- [ ] **Step 3: Refine the file-node rollup to be `Clean`-aware**

In `src/lib.rs::build_tree`, inside the inner `mk` function's file branch, change the file node's `rollup` so a `Clean` file contributes 0. Replace the current file-branch `Rollup { dirs_touched: 0, files_changed: 1, churn: f.churn.clone() }` with the version below (only `files_changed` changes):

```rust
        if let Some(f) = files.get(path) {
            TreeNode {
                name: path.file_name().unwrap().to_string_lossy().to_string(),
                path: path.display().to_string(),
                kind: NodeKind::File,
                status: f.status.clone(),
                churn: f.churn.clone(),
                rollup: Rollup {
                    dirs_touched: 0,
                    files_changed: if f.status == ChangeStatus::Clean { 0 } else { 1 },
                    churn: f.churn.clone(),
                },
                children: vec![],
            }
        } else {
```

- [ ] **Step 4: Refine directory rollup accumulation (count only dirs containing changes)**

In `src/lib.rs::build_tree`, in BOTH rollup accumulation loops (the one inside `mk` for a directory's children, and the summary loop near the end), replace:

```rust
                if c.kind == NodeKind::Directory {
                    r.dirs_touched += 1 + c.rollup.dirs_touched;
                }
```

with:

```rust
                if c.kind == NodeKind::Directory {
                    r.dirs_touched +=
                        c.rollup.dirs_touched + usize::from(c.rollup.files_changed > 0);
                }
```

and likewise in the summary loop replace:

```rust
        if c.kind == NodeKind::Directory {
            summary.dirs_touched += 1 + c.rollup.dirs_touched;
        }
```

with:

```rust
        if c.kind == NodeKind::Directory {
            summary.dirs_touched +=
                c.rollup.dirs_touched + usize::from(c.rollup.files_changed > 0);
        }
```

- [ ] **Step 5: Update `collect_changes` to use the helper**

In `src/lib.rs::collect_changes`, replace the final return:

```rust
    let (dirset, fmap) = files_to_maps(files);
    Ok(Some(build_tree(root_name, mode, View::BlastRadius, dirset, fmap, None)))
```

- [ ] **Step 6: Run the full suite (blast-radius behavior must be unchanged)**

Run: `cargo test`
Expected: PASS — all prior tests, including `test_default_fallback_wording_when_only_unstaged`, `test_mark_scheme_letter`, and `test_json_*`, stay green (blast-radius trees have no `Clean` files, so the refined rollups produce identical numbers).

- [ ] **Step 7: Full gate**

Run: `just all`
Expected: green.

- [ ] **Step 8: Commit**

```bash
git add src/lib.rs
git commit -m "refactor: build_tree takes prepared maps; rollups count only changed files/dirs"
```

---

## Task 5: Implement `collect_all_files` (walk + overlay)

**Files:**
- Modify: `src/lib.rs` (`WalkOpts`, `collect_all_files`)
- Test: `tests/cli.rs` (via the `--all` wiring in Task 6 — here we add a focused library-level check through the binary is deferred; this task adds the function and a unit test)

**Interfaces:**
- Consumes: `diff_files`, `add_untracked`, `files_to_maps` semantics, `build_tree(..., View::AllFiles, ...)`.
- Produces: `pub struct WalkOpts { pub all: bool, pub gitignore: bool, pub level: Option<usize>, pub dirs_only: bool }` and `pub fn collect_all_files(start: &Path, mode: ComparisonMode, opts: WalkOpts) -> anyhow::Result<Option<ChangeTree>>`.

- [ ] **Step 1: Write the failing unit test**

In `src/lib.rs`, add a test module at the end of the file:

```rust
#[cfg(test)]
mod all_files_tests {
    use super::*;
    use std::process::Command as Pcmd;

    fn git(dir: &Path, args: &[&str]) {
        Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
    }

    #[test]
    fn all_files_view_includes_unchanged_files() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::create_dir(p.join("src")).unwrap();
        std::fs::create_dir(p.join("docs")).unwrap();
        std::fs::write(p.join("src/changed.rs"), "a").unwrap();
        std::fs::write(p.join("docs/readme.md"), "b").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "init"]);
        std::fs::write(p.join("src/changed.rs"), "a2").unwrap();
        git(p, &["add", "src/changed.rs"]);

        let opts = WalkOpts { all: false, gitignore: false, level: None, dirs_only: false };
        let tree = collect_all_files(p, ComparisonMode::Staged, opts).unwrap().unwrap();
        assert_eq!(tree.view, View::AllFiles);

        // Collect every file name present in the tree.
        fn names(n: &TreeNode, out: &mut Vec<(String, ChangeStatus)>) {
            if n.kind == NodeKind::File {
                out.push((n.name.clone(), n.status.clone()));
            }
            for c in &n.children {
                names(c, out);
            }
        }
        let mut files = Vec::new();
        names(&tree.root, &mut files);
        assert!(files.iter().any(|(n, _)| n == "readme.md"), "unchanged file must appear");
        let changed = files.iter().find(|(n, _)| n == "changed.rs").expect("changed file present");
        assert_eq!(changed.1, ChangeStatus::Staged);
        let unchanged = files.iter().find(|(n, _)| n == "readme.md").unwrap();
        assert_eq!(unchanged.1, ChangeStatus::Clean);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib all_files_view_includes_unchanged_files`
Expected: FAIL — `collect_all_files` / `WalkOpts` do not exist (compile error).

- [ ] **Step 3: Add `WalkOpts` and `collect_all_files`**

In `src/lib.rs`, add (after `collect_default_with_fallback`):

```rust
#[derive(Debug, Clone, Copy)]
pub struct WalkOpts {
    pub all: bool,
    pub gitignore: bool,
    pub level: Option<usize>,
    pub dirs_only: bool,
}

pub fn collect_all_files(
    start: &Path,
    mode: ComparisonMode,
    opts: WalkOpts,
) -> anyhow::Result<Option<ChangeTree>> {
    let Ok(repo) = Repository::discover(start) else {
        return Ok(None);
    };
    let workdir =
        repo.workdir().ok_or_else(|| anyhow::anyhow!("bare repositories are not supported"))?;
    let scope = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let scope_rel = scope.strip_prefix(workdir).unwrap_or(Path::new("")).to_path_buf();

    // Build the git change map, re-keyed relative to the scope root.
    let mut changed = diff_files(&repo, &mode)?;
    add_untracked(&repo, &mut changed)?;
    let mut change_map: BTreeMap<PathBuf, FileChange> = BTreeMap::new();
    for mut f in changed {
        let rel = if scope_rel.as_os_str().is_empty() {
            Some(f.path.clone())
        } else {
            f.path.strip_prefix(&scope_rel).ok().map(|r| r.to_path_buf())
        };
        if let Some(rel) = rel {
            f.path = rel.clone();
            change_map.insert(rel, f);
        }
    }

    // Walk the filesystem; keys are paths relative to the scope root.
    let mut builder = ignore::WalkBuilder::new(start);
    builder.hidden(!opts.all).git_ignore(opts.gitignore);
    if let Some(level) = opts.level {
        builder.max_depth(Some(level));
    }
    let mut dirset: BTreeSet<PathBuf> = BTreeSet::new();
    let mut fmap: BTreeMap<PathBuf, FileChange> = BTreeMap::new();
    for result in builder.build() {
        let Ok(entry) = result else { continue };
        if entry.depth() == 0 {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(start) else { continue };
        let rel = rel.to_path_buf();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            dirset.insert(rel);
        } else {
            if opts.dirs_only {
                continue;
            }
            let fc = change_map.get(&rel).cloned().unwrap_or_else(|| FileChange {
                path: rel.clone(),
                status: ChangeStatus::Clean,
                churn: Churn::default(),
            });
            fmap.insert(rel, fc);
        }
    }

    let root_name = if scope_rel.as_os_str().is_empty() {
        workdir.file_name().and_then(|s| s.to_str()).unwrap_or(".").to_string()
    } else {
        scope_rel.display().to_string()
    };
    Ok(Some(build_tree(root_name, mode, View::AllFiles, dirset, fmap, None)))
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib all_files_view_includes_unchanged_files`
Expected: PASS.

- [ ] **Step 5: Full gate**

Run: `just all`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs
git commit -m "feat: collect_all_files walks the tree and overlays git change status"
```

---

## Task 6: Wire `--all`/`--tree` to the all-files view; generalize fallback wording

`--all` (and its alias `--tree`) now render the all-files view. The fallback banner wording is generalized so it reads correctly under either view.

**Files:**
- Modify: `src/app.rs` (make `--tree` an alias of `--all`)
- Modify: `src/main.rs` (dispatch all-files; pass `WalkOpts`)
- Modify: `src/lib.rs` (generalize fallback string)
- Test: `tests/cli.rs`

**Interfaces:**
- Consumes: `collect_all_files`, `WalkOpts` (Task 5).

- [ ] **Step 1: Write the failing test — `--all` shows unchanged files**

Append to `tests/cli.rs`:

```rust
#[test]
fn test_all_files_view_shows_unchanged_files() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::create_dir(p.join("src"))?;
    fs::create_dir(p.join("docs"))?;
    fs::write(p.join("src/changed.rs"), "a")?;
    fs::write(p.join("docs/readme.md"), "b")?;
    Command::new("git").args(["add", "."]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    fs::write(p.join("src/changed.rs"), "a2")?;
    Command::new("git").args(["add", "src/changed.rs"]).current_dir(p).output()?;

    for flag in ["--all", "--tree"] {
        let mut cmd = Command::cargo_bin("difftree")?;
        cmd.arg(flag).arg(p);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("changed.rs"))
            .stdout(predicate::str::contains("readme.md"));
    }
    Ok(())
}
```

- [ ] **Step 2: Write the failing test — all-files JSON marks unchanged `Clean`**

Append to `tests/cli.rs`:

```rust
#[test]
fn test_all_files_json_marks_unchanged_clean() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("unchanged.txt"), "x")?;
    fs::write(p.join("staged.txt"), "y")?;
    Command::new("git").args(["add", "unchanged.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    fs::write(p.join("staged.txt"), "y")?;
    Command::new("git").args(["add", "staged.txt"]).current_dir(p).output()?;

    let output = Command::cargo_bin("difftree")?.arg("--all").arg("--json").arg(p).output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("\"view\": \"all-files\""));
    assert!(stdout.contains("unchanged.txt"));
    assert!(stdout.contains("\"Clean\""));
    Ok(())
}
```

- [ ] **Step 3: Run both to verify they fail**

Run: `cargo test --test cli test_all_files_view_shows_unchanged_files test_all_files_json_marks_unchanged_clean`
Expected: FAIL — today `--all`/`--tree` only render changed files (`readme.md`/`unchanged.txt` absent); JSON `view` is `blast-radius`.

- [ ] **Step 4: Make `--tree` an alias of `--all` in `app.rs`**

In `src/app.rs`, remove the separate `tree` field and add `tree` as an alias on `all`:

```rust
    #[arg(long, alias = "tree")]
    pub all: bool,
```

(Delete the `pub tree: bool,` field. Search the codebase for `view_args.tree` / `.tree` references and update them in the next step.)

- [ ] **Step 5: Dispatch the all-files view in `main.rs`**

In `src/main.rs::run_cli`, update the `use_fallback` line to drop the now-removed `tree` field, and add the all-files dispatch. Replace the block from `let use_fallback = ...` through `let Some(tree) = tree else {...};` with:

```rust
    let use_fallback = !explicit_mode && !view_args.all && !view_args.ignored;
    let walk = difftree::WalkOpts {
        all: view_args.show_all,
        gitignore: view_args.gitignore,
        level: view_args.level,
        dirs_only: view_args.dirs_only,
    };
    let tree = if view_args.all {
        // All-files view honors the same staged -> unstaged fallback as the bare
        // default when no explicit comparison flag is given (spec §3).
        let mut t = difftree::collect_all_files(&view_args.path, mode.clone(), walk)?;
        if !explicit_mode && t.as_ref().is_some_and(|i| i.summary.files_changed == 0) {
            let mut u =
                difftree::collect_all_files(&view_args.path, ComparisonMode::Unstaged, walk)?;
            if let Some(ui) = &mut u {
                ui.fallback = Some("No staged changes — showing unstaged changes".to_string());
            }
            if u.is_some() {
                t = u;
            }
        }
        t
    } else if use_fallback {
        collect_default_with_fallback(&view_args.path)?
    } else {
        collect_changes(&view_args.path, mode, true)?
    };
    let Some(tree) = tree else {
        return view::run(view_args, ls_colors);
    };
```

Also update the `wants_plain_tree` expression earlier in `run_cli`: replace `|| !view_args.tree` with `|| !view_args.all` (the `tree` field no longer exists). The final condition must read:

```rust
    let wants_plain_tree = view_args.plain
        || view_args.git_status
        || (!view_args.json
            && !view_args.all
            && !view_args.unstaged
            && !view_args.uncommitted
            && view_args.range.is_none()
            && view_args.against.is_none()
            && !view_args.ignored
            && !is_git_repo(&view_args.path));
```

Also add `staged`-awareness so an explicit `--staged` inside a repo is not diverted to the plain tree — confirm `view_args.staged` is NOT part of `wants_plain_tree` (it stays git-aware via the default `mode = Staged`). No code needed beyond the block above; this step is a read-back verification.

- [ ] **Step 6: Remove the now-dead `import`/`matches!` for the staged-only shortcut**

In `src/main.rs::run_cli`, the previous `let tree = if matches!(mode, ComparisonMode::Staged) && ...` line was replaced in Task 2; confirm no remaining reference to `view_args.tree` exists:

Run: `grep -n "view_args.tree\|\.tree\b" src/main.rs`
Expected: no matches. If any remain, replace with `view_args.all`.

- [ ] **Step 7: Generalize the fallback wording in `lib.rs`**

In `src/lib.rs::collect_default_with_fallback`, change the fallback string so it is accurate for either view:

```rust
        t.fallback = Some("No staged changes — showing unstaged changes".to_string());
```

- [ ] **Step 8: Update the fallback-wording test expectation**

In `tests/cli.rs::test_default_fallback_wording_when_only_unstaged`, update the expected substring:

```rust
        .stdout(predicate::str::contains("No staged changes — showing unstaged changes"))
```

- [ ] **Step 9: Run the affected tests**

Run: `cargo test --test cli test_all_files_view_shows_unchanged_files test_all_files_json_marks_unchanged_clean test_default_fallback_wording_when_only_unstaged`
Expected: PASS (3 passed).

- [ ] **Step 10: Full suite + gate**

Run: `just all`
Expected: green — entire suite passes.

- [ ] **Step 11: Manual smoke check (the original bug)**

Run:
```bash
TMP=$(mktemp -d) && git -C "$TMP" init -q && git -C "$TMP" config user.email t@e.com && git -C "$TMP" config user.name T
mkdir -p "$TMP/src" "$TMP/docs" && echo a > "$TMP/src/c.rs" && echo b > "$TMP/docs/r.md"
git -C "$TMP" add -A && git -C "$TMP" commit -qm init && echo a2 > "$TMP/src/c.rs" && git -C "$TMP" add "$TMP/src/c.rs"
cargo run -q -- --all "$TMP"
```
Expected: output lists BOTH `docs/r.md` (no mark) and `src/c.rs` (with `●`), proving the all-files view works.

- [ ] **Step 12: Commit**

```bash
git add src/app.rs src/main.rs src/lib.rs tests/cli.rs
git commit -m "feat: --all/--tree render the all-files view with change overlay"
```

---

## Task 7: Update docs — naming inversion + CHANGELOG

**Files:**
- Modify: `docs/specs/difftree-decisions-v0.2.md`
- Modify: `docs/PRD/difftree-prd-v0.2.md`
- Modify: `CHANGELOG.md`

**Interfaces:** none (documentation).

- [ ] **Step 1: Update the comparison-precedence list in the decisions doc**

In `docs/specs/difftree-decisions-v0.2.md`, replace the precedence list and the note under "Comparison precedence" with:

```markdown
1. `--range <A..B>`
2. `--against <ref>`
3. `--uncommitted`
4. `--unstaged`
5. `--staged` (explicit; no auto-fallback)
6. default staged, with the documented unstaged fallback for the hero view

`tree` compatibility keeps `-a` as "show hidden files". The combined (HEAD vs working tree plus index) comparison is `--uncommitted`; `--all` (alias `--tree`) selects the all-files **view**, not a comparison.
```

- [ ] **Step 2: Update the flag table in the decisions doc**

In `docs/specs/difftree-decisions-v0.2.md`, replace the `--tree` and `--all` rows, and add the new comparison rows:

```markdown
| `--all`, `--tree` | All-files view: every file, with change marks overlaid (Clean when unchanged). |
| `--staged`, `--cached` | Compare index to HEAD (explicit; no auto-fallback). |
| `--unstaged` | Compare working tree to index. |
| `--uncommitted` | Compare HEAD to working tree plus index (staged + unstaged + untracked). |
```

(Remove the old standalone `--all` comparison row and the old `--tree` row.)

- [ ] **Step 3: Note the inversion in the PRD**

In `docs/PRD/difftree-prd-v0.2.md` §6.4, append a resolution note after the existing `[DETAIL]` text:

```markdown
  - **Resolved (2026-06-21):** `--all` (alias `--tree`) names the **all-files view**; the combined staged+unstaged **comparison** is `--uncommitted`; `--staged`/`--cached` is the explicit staged comparison. This inverts the earlier `--every-file` suggestion. See `docs/superpowers/specs/2026-06-21-view-and-comparison-flags-design.md`.
```

- [ ] **Step 4: Update the JSON schema bullet list in the decisions doc**

In `docs/specs/difftree-decisions-v0.2.md` under "JSON schema", add a bullet after the `comparison` bullet:

```markdown
- `view`: the active view, `"blast-radius"` or `"all-files"`.
```

- [ ] **Step 5: Add a CHANGELOG entry**

In `CHANGELOG.md`, under `## Unreleased`, add:

```markdown
### Added

- All-files view (`--all`, alias `--tree`): renders the complete directory tree
  with git change marks overlaid; unchanged files are shown as `Clean`.
- Explicit `--staged` (alias `--cached`) comparison flag.
- `--uncommitted` comparison (staged + unstaged + untracked vs HEAD), renamed
  from the previous `--all` comparison meaning.
- `view` field (`blast-radius` | `all-files`) in the `--json` model.

### Changed

- `--all` no longer means the combined comparison; use `--uncommitted` for that.
- `--tree` now shows every file (previously it rendered only changed files).
```

- [ ] **Step 6: Commit**

```bash
git add docs/specs/difftree-decisions-v0.2.md docs/PRD/difftree-prd-v0.2.md CHANGELOG.md
git commit -m "docs: record --all/--uncommitted naming inversion and all-files view"
```

---

## Self-Review (completed by plan author)

**Spec coverage:**
- View/comparison split → Tasks 2, 3, 6. ✔
- `--all` = all-files view, `--tree` alias → Task 6. ✔
- `--uncommitted` rename → Tasks 1, 2. ✔
- `--staged`/`--cached` explicit, no fallback → Task 2. ✔
- Approach A walk+overlay → Task 5. ✔
- `Clean` unchanged files + correct rollups → Task 4. ✔
- JSON `view` field + `Uncommitted` value → Tasks 1, 3, 6. ✔
- Collision (`-a` stays show-hidden; `--all` long-only) → Task 6 (no `-a` short added). ✔
- Generalized fallback wording → Task 6. ✔
- Docs/PRD inversion → Task 7. ✔
- Out-of-scope (churn counts, heat, ignored, `-P/-I/prune`, full sort plumbing) → not implemented, called out in spec §9. ✔

**Placeholder scan:** Task 4 Step 3 intentionally shows scratch lines then directs their removal — the final state is unambiguous. No other TODO/TBD/“handle edge cases”. ✔

**Type consistency:** `ComparisonMode::Uncommitted`, `View::{BlastRadius,AllFiles}`, `build_tree(root_name, mode, view, dirset, fmap, fallback)`, `files_to_maps`, `WalkOpts{all,gitignore,level,dirs_only}`, `collect_all_files(start, mode, opts)` are used identically across Tasks 1–6. ✔
