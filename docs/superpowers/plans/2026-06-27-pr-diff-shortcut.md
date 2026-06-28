# `--pr` PR-style Diff Shortcut Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `--pr` CLI flag that shows the GitHub-PR diff for the current branch — everything changed since it diverged from the base branch (the merge-base), with a `--committed` modifier to narrow to committed branch commits only.

**Architecture:** A new first-class `ComparisonMode::Pr { merge_base, committed }` reuses the two git2 diff paths that already back `--against` (tree→workdir) and `--range` (tree→tree). A new `resolve_pr_base` helper resolves the base ref (auto-detect `origin/HEAD` default → `main` → `master`, preferring `origin/<name>` over local) and computes `merge-base(base, HEAD)`. `main.rs` resolves the base, warns when run on the base branch, and constructs the mode. CLI-only; the interactive TUI is a separate follow-up.

**Tech Stack:** Rust, `git2` 0.20 (default-features off), `clap` 4.5 derive, `anyhow`; tests use `assert_cmd` + `predicates` + `tempfile` (integration) and `std::process::Command` against temp git repos (unit).

## Global Constraints

- No new crate dependencies. Use existing `git2`, `clap`, `anyhow`.
- `git2` is built with `default-features = false` — do not rely on networking features (the remote-preference logic reads local `refs/remotes/origin/*`; never fetches).
- JSON `schema_version` stays `"difftree.v1"` (pre-release; the new `Pr` enum variant may appear in `"comparison"` output — acceptable, no compat guarantee).
- `--pr` is mutually exclusive with `--range`, `--against`, `--staged`, `--unstaged`, `--uncommitted` (clap-enforced). `--committed` requires `--pr`.
- Endpoints: default = `merge-base → working tree` (includes untracked); `--committed` = `merge-base → HEAD` (excludes untracked).
- Errors are hard errors (non-zero exit) for: unresolvable/bad base, no merge-base, outside a git repo. On the base branch: warn to stderr and proceed.
- Commit messages follow Conventional Commits.
- Version bump (to v0.3.0) and `Cargo.toml`/tag reconciliation are handled at release time — NOT in these tasks.
- Verify each task with the exact `cargo test` command shown; run `just lint` (clippy `-D warnings`) before each commit.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src/lib.rs` | Core model + git collection | Add `ComparisonMode::Pr` variant; add `Pr` arm in `diff_files`; fix untracked guard in `collect_all_files`; add `PrBase` + `resolve_pr_base` + `origin_default_branch`; add `#[cfg(test)] mod pr_tests`. |
| `src/app.rs` | CLI arg definitions | Add `pr: Option<Option<String>>` and `committed: bool` to `ViewArgs`. |
| `src/main.rs` | CLI dispatch | Resolve base, warn on-base, slot `Pr` into mode selection / `explicit_mode` / `wants_plain_tree`; set `include_untracked = !committed`. |
| `tests/cli.rs` | Binary integration tests | Add clap-validation tests + end-to-end `--pr` behavior tests. |
| `README.md` | User docs | Document `--pr` / `--pr <ref>` / `--committed`. |

---

## Task 1: `ComparisonMode::Pr` mode + merge-base diff engine

Add the new comparison mode and make both collection entry points (`collect_changes`, `collect_all_files`) diff against a supplied merge-base SHA. This task takes the merge-base SHA as a given (resolution is Task 2), so it is fully testable in isolation.

**Files:**
- Modify: `src/lib.rs:11-17` (enum), `src/lib.rs:238-264` (`diff_files`), `src/lib.rs:340-343` (`collect_all_files` untracked guard)
- Test: `src/lib.rs` (new `#[cfg(test)] mod pr_tests`)

**Interfaces:**
- Produces: `ComparisonMode::Pr { merge_base: String, committed: bool }` — `merge_base` is a commit SHA string; consumed by `diff_files`. Used by Task 4.
- Consumes: existing `collect_changes(start, mode, include_untracked) -> Result<Option<ChangeTree>>` and `collect_all_files(start, mode, WalkOpts) -> Result<Option<ChangeTree>>`.

- [ ] **Step 1: Write the failing unit tests**

Append to `src/lib.rs`:

```rust
#[cfg(test)]
mod pr_tests {
    use super::*;
    use std::process::Command as Pcmd;

    fn git(dir: &Path, args: &[&str]) {
        Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
    }
    fn git_out(dir: &Path, args: &[&str]) -> String {
        let o = Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
        String::from_utf8(o.stdout).unwrap().trim().to_string()
    }
    fn file_names(tree: &ChangeTree) -> Vec<String> {
        fn walk(n: &TreeNode, out: &mut Vec<String>) {
            if n.kind == NodeKind::File {
                out.push(n.name.clone());
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        let mut v = Vec::new();
        walk(&tree.root, &mut v);
        v
    }

    /// Sets up: c0 (base.txt) on base branch; a `feature` branch with feat.txt;
    /// a base-only commit main2.txt that feature never sees; and an untracked
    /// working.txt in feature's worktree. Returns (tmpdir, c0_sha).
    fn setup_pr_repo() -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("base.txt"), "x").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        let base_branch = git_out(p, &["symbolic-ref", "--short", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("feat.txt"), "y").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feat"]);
        git(p, &["checkout", &base_branch]);
        std::fs::write(p.join("main2.txt"), "z").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "main2"]);
        git(p, &["checkout", "feature"]);
        std::fs::write(p.join("working.txt"), "w").unwrap(); // untracked
        (tmp, c0)
    }

    #[test]
    fn pr_committed_shows_only_branch_commits() {
        let (tmp, c0) = setup_pr_repo();
        let mode = ComparisonMode::Pr { merge_base: c0, committed: true };
        let tree = collect_changes(tmp.path(), mode, false).unwrap().unwrap();
        let names = file_names(&tree);
        assert!(names.iter().any(|n| n == "feat.txt"), "branch commit shown");
        assert!(!names.iter().any(|n| n == "main2.txt"), "base-only commit excluded");
        assert!(!names.iter().any(|n| n == "working.txt"), "uncommitted excluded");
    }

    #[test]
    fn pr_default_includes_working_tree_and_untracked() {
        let (tmp, c0) = setup_pr_repo();
        let mode = ComparisonMode::Pr { merge_base: c0, committed: false };
        let tree = collect_changes(tmp.path(), mode, true).unwrap().unwrap();
        let names = file_names(&tree);
        assert!(names.iter().any(|n| n == "feat.txt"), "branch commit shown");
        assert!(names.iter().any(|n| n == "working.txt"), "untracked shown");
        assert!(!names.iter().any(|n| n == "main2.txt"), "base-only commit excluded");
    }

    #[test]
    fn pr_all_view_excludes_untracked_when_committed() {
        let (tmp, c0) = setup_pr_repo();
        let opts = WalkOpts { all: false, gitignore: false, level: None, dirs_only: false };
        let mode = ComparisonMode::Pr { merge_base: c0, committed: true };
        let tree = collect_all_files(tmp.path(), mode, opts).unwrap().unwrap();
        let names = file_names(&tree);
        // all-files view lists every file, but untracked must NOT appear under --committed
        assert!(names.iter().any(|n| n == "base.txt"), "unchanged file listed in all-files view");
        assert!(!names.iter().any(|n| n == "working.txt"), "untracked excluded under committed");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib pr_tests`
Expected: FAIL — compile error, `no variant named Pr found for enum ComparisonMode`.

- [ ] **Step 3: Add the `Pr` enum variant**

In `src/lib.rs`, change the enum (lines 11-17) to:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComparisonMode {
    Staged,
    Unstaged,
    Uncommitted,
    Range { range: String },
    Against { reference: String },
    Pr { merge_base: String, committed: bool },
}
```

- [ ] **Step 4: Add the `Pr` arm to `diff_files`**

In `src/lib.rs`, inside the `match mode` in `diff_files` (after the `ComparisonMode::Range` arm, before the closing `}` at line 263), add:

```rust
        ComparisonMode::Pr { merge_base, committed } => {
            let mb_tree = repo.revparse_single(merge_base)?.peel_to_tree()?;
            if *committed {
                let head_tree = repo.head()?.peel_to_tree()?;
                repo.diff_tree_to_tree(Some(&mb_tree), Some(&head_tree), Some(&mut opts))?
            } else {
                repo.diff_tree_to_workdir_with_index(Some(&mb_tree), Some(&mut opts))?
            }
        }
```

- [ ] **Step 5: Fix the untracked guard in `collect_all_files`**

In `src/lib.rs`, replace lines 341-343:

```rust
    if !matches!(mode, ComparisonMode::Range { .. }) {
        add_untracked(&repo, &mut changed)?;
    }
```

with:

```rust
    let include_untracked = match &mode {
        ComparisonMode::Range { .. } => false,
        ComparisonMode::Pr { committed, .. } => !committed,
        _ => true,
    };
    if include_untracked {
        add_untracked(&repo, &mut changed)?;
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib pr_tests`
Expected: PASS (3 tests).

- [ ] **Step 7: Lint, then commit**

Run: `just lint` → expect no warnings.

```bash
git add src/lib.rs
git commit -m "feat: add ComparisonMode::Pr merge-base diff (default + --committed)"
```

---

## Task 2: `resolve_pr_base` base + merge-base resolution

Add the helper that picks the base ref (auto-detect, remote-preferred, overridable) and computes the merge-base + on-base flag.

**Files:**
- Modify: `src/lib.rs` (add `PrBase`, `resolve_pr_base`, `origin_default_branch` near `collect_changes`)
- Test: `src/lib.rs` (extend `mod pr_tests`)

**Interfaces:**
- Produces:
  ```rust
  pub struct PrBase {
      pub base_name: String,  // e.g. "main"
      pub base_ref: String,   // e.g. "origin/main" — the ref actually used
      pub merge_base: String, // merge-base commit SHA
      pub on_base: bool,      // merge_base == HEAD (no divergence)
  }
  pub fn resolve_pr_base(start: &Path, base_override: Option<&str>) -> anyhow::Result<PrBase>
  ```
  Consumed by Task 4 (`main.rs`).

- [ ] **Step 1: Write the failing unit tests**

Add these tests inside `mod pr_tests` in `src/lib.rs` (reuse its `git`/`git_out` helpers):

```rust
    /// Repo with a `main` branch (c0) and a `feature` branch (c0 + feat).
    /// Returns (tmpdir, c0_sha).
    fn setup_named_repo(base: &str) -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("base.txt"), "x").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", base]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("feat.txt"), "y").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feat"]);
        (tmp, c0)
    }

    #[test]
    fn resolve_auto_detects_main() {
        let (tmp, c0) = setup_named_repo("main");
        let b = resolve_pr_base(tmp.path(), None).unwrap();
        assert_eq!(b.base_name, "main");
        assert_eq!(b.merge_base, c0);
        assert!(!b.on_base);
    }

    #[test]
    fn resolve_falls_through_to_master() {
        let (tmp, _c0) = setup_named_repo("master");
        let b = resolve_pr_base(tmp.path(), None).unwrap();
        assert_eq!(b.base_name, "master");
    }

    #[test]
    fn resolve_bad_override_errors() {
        let (tmp, _c0) = setup_named_repo("main");
        let err = resolve_pr_base(tmp.path(), Some("no-such-ref")).unwrap_err();
        assert!(err.to_string().contains("could not resolve base branch"));
    }

    #[test]
    fn resolve_on_base_branch_sets_flag() {
        let (tmp, c0) = setup_named_repo("main");
        git(tmp.path(), &["checkout", "main"]);
        let b = resolve_pr_base(tmp.path(), None).unwrap();
        assert_eq!(b.merge_base, c0);
        assert!(b.on_base);
    }

    #[test]
    fn resolve_prefers_remote_tracking_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("base.txt"), "x").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", "main"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        // Fabricate a remote-tracking ref at c0 without a real remote.
        git(p, &["update-ref", "refs/remotes/origin/main", &c0]);
        // Advance local main past origin/main.
        std::fs::write(p.join("base.txt"), "x2").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c1"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("feat.txt"), "y").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feat"]);

        let b = resolve_pr_base(p, None).unwrap();
        assert_eq!(b.base_ref, "origin/main", "remote-tracking ref preferred over local");
        assert_eq!(b.merge_base, c0, "merge-base taken against origin/main (c0)");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib pr_tests::resolve`
Expected: FAIL — `cannot find function resolve_pr_base in this scope`.

- [ ] **Step 3: Implement `PrBase`, `resolve_pr_base`, `origin_default_branch`**

In `src/lib.rs`, add (place directly above `pub fn collect_changes`, near line 188):

```rust
/// The resolved base for a `--pr` comparison.
pub struct PrBase {
    /// The base branch's short name (for messages), e.g. "main".
    pub base_name: String,
    /// The ref actually used (remote-preferred), e.g. "origin/main".
    pub base_ref: String,
    /// The merge-base commit SHA between the base and HEAD.
    pub merge_base: String,
    /// True when the merge-base equals HEAD (no divergence from base).
    pub on_base: bool,
}

/// Reads the default branch name from `refs/remotes/origin/HEAD`, if present.
fn origin_default_branch(repo: &Repository) -> Option<String> {
    let r = repo.find_reference("refs/remotes/origin/HEAD").ok()?;
    let target = r.symbolic_target()?; // e.g. "refs/remotes/origin/main"
    target.strip_prefix("refs/remotes/origin/").map(|s| s.to_string())
}

/// Resolves the base branch for `--pr` and computes the merge-base with HEAD.
///
/// Base candidates (auto-detect): the `origin/HEAD` default branch, then `main`,
/// then `master`; an explicit `base_override` replaces the candidate list. Each
/// candidate prefers its remote-tracking ref (`origin/<name>`) over the local
/// branch. Errors if no candidate resolves or there is no common history.
pub fn resolve_pr_base(start: &Path, base_override: Option<&str>) -> anyhow::Result<PrBase> {
    let repo = Repository::discover(start)
        .map_err(|_| anyhow::anyhow!("difftree: --pr requires a git repository"))?;

    let mut candidates: Vec<String> = Vec::new();
    if let Some(o) = base_override {
        candidates.push(o.to_string());
    } else {
        if let Some(def) = origin_default_branch(&repo) {
            candidates.push(def);
        }
        candidates.push("main".to_string());
        candidates.push("master".to_string());
    }
    let mut seen = BTreeSet::new();
    candidates.retain(|c| seen.insert(c.clone()));

    let mut resolved: Option<(String, String, git2::Oid)> = None;
    for name in &candidates {
        for cand_ref in [format!("origin/{name}"), name.clone()] {
            if let Ok(obj) = repo.revparse_single(&cand_ref) {
                if let Ok(commit) = obj.peel_to_commit() {
                    resolved = Some((name.clone(), cand_ref, commit.id()));
                    break;
                }
            }
        }
        if resolved.is_some() {
            break;
        }
    }
    let (base_name, base_ref, base_oid) = resolved.ok_or_else(|| {
        anyhow::anyhow!(
            "difftree: could not resolve base branch (tried: {}); pass one with --pr <ref>",
            candidates.join(", ")
        )
    })?;

    let head_oid = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|e| anyhow::anyhow!("difftree: cannot read HEAD: {e}"))?
        .id();
    let mb = repo.merge_base(base_oid, head_oid).map_err(|_| {
        anyhow::anyhow!("difftree: no common history between HEAD and base '{base_name}'")
    })?;

    Ok(PrBase {
        base_name,
        base_ref,
        merge_base: mb.to_string(),
        on_base: mb == head_oid,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib pr_tests`
Expected: PASS (8 tests total — 3 from Task 1, 5 here).

- [ ] **Step 5: Lint, then commit**

Run: `just lint` → expect no warnings.

```bash
git add src/lib.rs
git commit -m "feat: add resolve_pr_base base/merge-base resolution"
```

---

## Task 3: `--pr` / `--committed` CLI flags

Add the flags to `ViewArgs` with clap conflict/requires rules. main.rs does not consume them yet; this task only proves parsing/validation.

**Files:**
- Modify: `src/app.rs:45-48` (add fields after `against`)
- Test: `tests/cli.rs` (add two validation tests)

**Interfaces:**
- Produces: `ViewArgs.pr: Option<Option<String>>` (`None` = absent, `Some(None)` = `--pr`, `Some(Some(ref))` = `--pr <ref>`) and `ViewArgs.committed: bool`. Consumed by Task 4.

- [ ] **Step 1: Write the failing tests**

Append to `tests/cli.rs`:

```rust
#[test]
fn pr_committed_requires_pr() {
    let mut cmd = Command::cargo_bin("difftree").unwrap();
    cmd.arg("--committed");
    cmd.assert().failure();
}

#[test]
fn pr_conflicts_with_against() {
    let mut cmd = Command::cargo_bin("difftree").unwrap();
    cmd.arg("--against").arg("main").arg("--pr");
    cmd.assert().failure();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli pr_`
Expected: FAIL — both assert `.failure()` but the binary currently accepts these args (unknown `--pr`/`--committed` would actually error today as *unknown* args, so to be precise: run and confirm; if they pass trivially because the args are unknown, proceed anyway — Step 4 makes them known-but-validated). The meaningful check is Step 4's PASS after the rules exist.

> Note: before the flags exist, `--committed`/`--pr` are *unknown* args → clap already fails. After Step 3 they are *known* args governed by `requires`/`conflicts_with_all`, so the failure is now semantic. Both states satisfy `.failure()`; the value of these tests is locking the behavior once the flags are real.

- [ ] **Step 3: Add the flags**

In `src/app.rs`, immediately after the `against` field (line 48), add:

```rust
    #[arg(
        long,
        value_name = "REF",
        num_args = 0..=1,
        conflicts_with_all = ["range", "against", "staged", "unstaged", "uncommitted"]
    )]
    pub pr: Option<Option<String>>,
    #[arg(long, requires = "pr")]
    pub committed: bool,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test cli pr_`
Expected: PASS (2 tests). `--committed` alone → "required" error; `--against main --pr` → "cannot be used with" error.

- [ ] **Step 5: Lint, then commit**

Run: `just lint` → expect no warnings.

```bash
git add src/app.rs tests/cli.rs
git commit -m "feat: add --pr/--committed CLI flags with conflicts"
```

---

## Task 4: Wire `--pr` into the CLI dispatch

Resolve the base, emit the on-base warning, construct `ComparisonMode::Pr`, and integrate with the `explicit_mode` / `wants_plain_tree` / `include_untracked` logic.

**Files:**
- Modify: `src/main.rs:15-19` (imports), `src/main.rs:47-101` (guard + mode selection)
- Test: `tests/cli.rs` (end-to-end behavior tests)

**Interfaces:**
- Consumes: `difftree::resolve_pr_base`, `difftree::PrBase`, `ComparisonMode::Pr { merge_base, committed }` (Tasks 1-2); `ViewArgs.pr`, `ViewArgs.committed` (Task 3).

- [ ] **Step 1: Write the failing tests**

Append to `tests/cli.rs`:

```rust
use std::path::Path as StdPath;

fn git_in(dir: &StdPath, args: &[&str]) {
    std::process::Command::new("git").args(args).current_dir(dir).output().unwrap();
}

/// main (base.txt @ c0) → feature (feat.txt) ; base advances (main2.txt) ;
/// back on feature with an untracked working.txt.
fn make_pr_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path();
    git_in(p, &["init"]);
    git_in(p, &["config", "user.email", "t@e.com"]);
    git_in(p, &["config", "user.name", "T"]);
    std::fs::write(p.join("base.txt"), "x").unwrap();
    git_in(p, &["add", "."]);
    git_in(p, &["commit", "-m", "c0"]);
    git_in(p, &["branch", "-M", "main"]);
    git_in(p, &["checkout", "-b", "feature"]);
    std::fs::write(p.join("feat.txt"), "y").unwrap();
    git_in(p, &["add", "."]);
    git_in(p, &["commit", "-m", "feat"]);
    git_in(p, &["checkout", "main"]);
    std::fs::write(p.join("main2.txt"), "z").unwrap();
    git_in(p, &["add", "."]);
    git_in(p, &["commit", "-m", "main2"]);
    git_in(p, &["checkout", "feature"]);
    std::fs::write(p.join("working.txt"), "w").unwrap(); // untracked
    tmp
}

#[test]
fn pr_default_shows_branch_and_working_not_base() {
    let tmp = make_pr_repo();
    Command::cargo_bin("difftree")
        .unwrap()
        .arg("--pr")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("feat.txt"))
        .stdout(predicate::str::contains("working.txt"))
        .stdout(predicate::str::contains("main2.txt").not());
}

#[test]
fn pr_committed_excludes_working_tree() {
    let tmp = make_pr_repo();
    Command::cargo_bin("difftree")
        .unwrap()
        .arg("--pr")
        .arg("--committed")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("feat.txt"))
        .stdout(predicate::str::contains("working.txt").not())
        .stdout(predicate::str::contains("main2.txt").not());
}

#[test]
fn pr_all_lists_unchanged_files() {
    let tmp = make_pr_repo();
    Command::cargo_bin("difftree")
        .unwrap()
        .arg("--pr")
        .arg("--all")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("base.txt"));
}

#[test]
fn pr_on_base_branch_warns() {
    let tmp = make_pr_repo();
    git_in(tmp.path(), &["checkout", "main"]);
    Command::cargo_bin("difftree")
        .unwrap()
        .arg("--pr")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("on base branch"));
}

#[test]
fn pr_bad_ref_errors() {
    let tmp = make_pr_repo();
    Command::cargo_bin("difftree")
        .unwrap()
        .arg("--pr")
        .arg("does-not-exist-xyz")
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not resolve base branch"));
}
```

> If `tests/cli.rs` does not already have `use predicates::prelude::*;` at the top, add it. Confirm the top of the file imports `assert_cmd::Command` and `tempfile::tempdir`/`tempfile` (it uses them already).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli pr_default_shows_branch_and_working_not_base`
Expected: FAIL — `--pr` is parsed but ignored, so output uses the default staged/unstaged path; `feat.txt` may be absent / `main2.txt` handling wrong. (At minimum the assertions do not hold.)

- [ ] **Step 3: Add the import**

In `src/main.rs`, change the `use difftree::{...}` block (lines 15-19) to add `resolve_pr_base`:

```rust
use difftree::{
    collect_all_files, collect_all_files_default_with_fallback, collect_changes,
    collect_default_with_fallback, resolve_pr_base, ComparisonMode, JsonRenderer, OutputFormat,
    Renderer, TerminalRenderer,
};
```

- [ ] **Step 4: Add `pr` to the plain-tree guard**

In `src/main.rs`, in the `wants_plain_tree` expression (lines 47-57), add `&& view_args.pr.is_none()` to the `!view_args.json && ...` chain. The block becomes:

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
            && !is_git_repo(&view_args.path));
```

- [ ] **Step 5: Resolve base + build the mode**

In `src/main.rs`, replace the mode-selection block (lines 67-83) with:

```rust
    let pr_base = if let Some(pr_opt) = &view_args.pr {
        let base = resolve_pr_base(&view_args.path, pr_opt.as_deref())?;
        if base.on_base {
            eprintln!(
                "difftree: on base branch '{}'; showing uncommitted changes only",
                base.base_name
            );
        }
        Some(base)
    } else {
        None
    };

    let mode = if let Some(base) = &pr_base {
        ComparisonMode::Pr { merge_base: base.merge_base.clone(), committed: view_args.committed }
    } else if let Some(range) = &view_args.range {
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
        || view_args.against.is_some()
        || view_args.pr.is_some();
```

- [ ] **Step 6: Set `include_untracked` for the Pr path**

In `src/main.rs`, in the final `else` branch of the tree-building block (currently line 99), replace:

```rust
        let include_untracked = !matches!(mode, ComparisonMode::Range { .. });
```

with:

```rust
        let include_untracked = match &mode {
            ComparisonMode::Range { .. } => false,
            ComparisonMode::Pr { committed, .. } => !committed,
            _ => true,
        };
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --test cli pr_`
Expected: PASS (7 tests — 2 from Task 3, 5 here).

- [ ] **Step 8: Run the full suite + lint**

Run: `cargo test` → expect all green.
Run: `just lint` → expect no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/main.rs tests/cli.rs
git commit -m "feat: wire --pr PR-style diff into the CLI"
```

---

## Task 5: Document `--pr` in the README

**Files:**
- Modify: `README.md:284`

- [ ] **Step 1: Update the comparison-modes bullet**

In `README.md`, replace line 284:

```markdown
- Comparison modes: `--unstaged`, `--all`, `--range <A..B>`, and `--against <ref>`.
```

with:

```markdown
- Comparison modes: `--unstaged`, `--all`, `--range <A..B>`, and `--against <ref>`.
- `--pr [<ref>]` shows the PR-style diff for the current branch: everything changed since it diverged from the base (the merge-base). The base auto-detects (`origin` default → `main` → `master`, preferring the `origin/<name>` remote ref); pass `--pr <ref>` to override. Default endpoint is the working tree (commits + staged + unstaged + untracked); add `--committed` to narrow to committed branch commits only (`merge-base → HEAD`).
```

- [ ] **Step 2: Verify the doc builds / no broken table**

Run: `cargo build` (sanity; docs are markdown only) and visually confirm the bullet renders.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document --pr PR-style diff shortcut"
```

---

## Self-Review

**Spec coverage:**
- Flag surface (`--pr [<ref>]`, `--committed`, conflicts/requires) → Task 3. ✓
- Base resolution (auto-detect order, remote-preferred, override) → Task 2 (`resolve_pr_base`) + tests. ✓
- Diff endpoints (default workdir incl. untracked; `--committed` tree→HEAD excl. untracked) → Task 1 (`diff_files` arm + untracked guards) + Task 4 (`include_untracked`). ✓
- `main.rs` wiring (explicit_mode, wants_plain_tree, on-base warning) → Task 4. ✓
- Error handling (bad/unresolvable base, no merge-base, outside repo = hard error; on-base = warn) → Task 2 (errors) + Task 4 (warning + integration tests `pr_bad_ref_errors`). ✓
- Composition with `--all`/`--json` → Task 1 (`collect_all_files`) + Task 4 (`pr_all_lists_unchanged_files`). ✓
- README → Task 5. ✓
- Out-of-scope (TUI, `--against`/`--range` semantics, version bump) → untouched. ✓

**Placeholder scan:** No TBD/TODO/"handle edge cases". Every code step shows complete code. ✓

**Type consistency:** `ComparisonMode::Pr { merge_base: String, committed: bool }` is identical across Task 1 (def + match arms), Task 4 (construction + `include_untracked` match). `PrBase` fields (`base_name`, `base_ref`, `merge_base`, `on_base`) are identical across Task 2 (def + tests) and Task 4 (usage of `.merge_base`, `.base_name`, `.on_base`). `resolve_pr_base(start, base_override) -> Result<PrBase>` signature matches between Task 2 and Task 4's `resolve_pr_base(&view_args.path, pr_opt.as_deref())`. ✓
