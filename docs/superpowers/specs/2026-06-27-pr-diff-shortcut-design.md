# difftree — `--pr` PR-style diff shortcut (CLI)

**Date:** 2026-06-27
**Status:** Design approved — ready for implementation plan
**Branch:** `feat/pr-diff-shortcut`
**Related:** `docs/superpowers/specs/2026-06-21-view-and-comparison-flags-design.md` (the comparison-mode model this builds on)

---

## 1. Problem

When reviewing your own work on a branch cut from `main`, the question is almost always
*"what does my PR change relative to main?"* — i.e. the GitHub PR diff. difftree can't
answer that today:

- **`--against main`** runs `diff_tree_to_workdir_with_index(main_tree, …)` — a plain
  two-dot diff of `main`'s tip against your working tree. If `main` advanced after you
  branched, files changed *on main* (that your branch never touched) leak into the output.
  It does **not** find where your branch diverged.
- **`--range A..B`** splits on the first `..` and does `diff_tree_to_tree` (two-dot). Passing
  `main...HEAD` (three-dot) breaks: it splits to `("main", ".HEAD")` and `revparse_single(".HEAD")`
  fails. No merge-base is ever computed.

No existing mode computes the **merge-base** — the commit where your branch diverged from the
base. That merge-base is exactly *"how far back to go."*

## 2. Core idea

Add a first-class **`--pr`** comparison mode whose base is `merge-base(base, HEAD)`
(GitHub PR semantics, `base...HEAD`). It reuses the comparison-mode model from the v0.2
design: `--pr` selects *what counts as changed*; it composes with the existing *views*
(blast-radius / `--all`) and renderers (`--json`, `--marks`, etc.).

Two endpoints, one default:

- **default** — `merge-base → working tree`: every change not yet on the base
  (branch commits **+** staged **+** unstaged **+** untracked).
- **`--committed`** — `merge-base → HEAD`: only the commits your branch added.

## 3. Flag surface (`src/app.rs` → `ViewArgs`)

| Flag | Type | Meaning |
|---|---|---|
| `--pr[=<ref>]` | `pr: Option<Option<String>>`, `num_args(0..=1)`, `require_equals(true)` | Enable PR mode. No value → auto-detect base; `--pr=develop` → override base ref. A following token remains the positional path scope. |
| `--pr-base <ref>` | `pr_base: Option<String>`, `requires("pr")` | Long-form explicit base override, useful with path scopes: `--pr --pr-base develop src`. |
| `--committed` | `bool`, `requires("pr")` | Narrow endpoint to `merge-base → HEAD` (committed branch commits only). |

- `--pr` is mutually exclusive with the other comparison flags:
  `conflicts_with_all = ["range", "against", "staged", "unstaged", "uncommitted"]` (clap-enforced).
- Inline `--pr=<ref>` and `--pr-base <ref>` are mutually exclusive by runtime validation.
- `--committed` without `--pr` is a clap error (`requires`).
- Composes freely with views/format flags: `--all`, `--json`, `--marks`, `-L`, `-d`, sorting.
- Composes with path scopes: `difftree --pr src`, `difftree --pr=main src`, and
  `difftree --pr --pr-base main src`.

## 4. Base resolution — new `lib.rs` helper

```
pub struct PrBase {
    pub base_name: String,      // e.g. "main"        (for messages)
    pub base_ref: String,       // e.g. "origin/main" (the ref actually used)
    pub merge_base: String,     // merge-base commit SHA
    pub on_base: bool,          // merge_base == HEAD  → no divergence
}

pub fn resolve_pr_base(start: &Path, base_override: Option<&str>) -> anyhow::Result<PrBase>
```

Algorithm:

1. **Candidate base names** (in order, deduped):
   - override given by `--pr=<ref>` or `--pr-base <ref>` → resolve the exact ref first
   - else → `[<origin/HEAD default branch>, "main", "master"]`
     (the origin default is read from the `origin/HEAD` symbolic ref when present).
2. **Resolve each auto-detected candidate, preferring the remote:** for each name try
   `origin/<name>`, then local `<name>`; the first that resolves wins and fixes
   `base_ref` / `base_name`. Explicit overrides use the exact ref first, then fall back to
   `origin/<name>` only for short branch names that do not resolve locally.
3. If **no** candidate resolves → hard error:
   `could not resolve base branch (tried: …); pass one with --pr=<ref> or --pr-base <ref>`.
4. Compute `merge_base = repo.merge_base(base_commit, HEAD)`. Unrelated histories
   (no common ancestor) → hard error. Set `on_base = (merge_base == HEAD)`.

## 5. Diff — new `ComparisonMode::Pr { merge_base: String, committed: bool }`

`main.rs` calls `resolve_pr_base` first, then constructs the mode carrying the resolved
`merge_base` SHA. The new arm in `diff_files` reuses the two git2 paths that already back
`--against` and `--range`:

- **default** → `diff_tree_to_workdir_with_index(merge_base_tree, …)` — includes untracked.
- **`--committed`** → `diff_tree_to_tree(merge_base_tree, HEAD_tree, …)` — excludes untracked.

Status mapping reuses the existing `diff.foreach` logic unchanged.

## 6. `main.rs` wiring

- Add `pr` to the `wants_plain_tree` exclusion list and to `explicit_mode`
  (so `--pr` errors outside a git repo rather than silently degrading to a plain tree).
- Call `resolve_pr_base`; if `on_base`, `eprintln!` the warning
  (`difftree: on base branch '<base_name>'; showing uncommitted changes only`) and proceed.
- Build `ComparisonMode::Pr { merge_base, committed }`; set `include_untracked = !committed`.
- All-files path (`collect_all_files`) and blast-radius path (`collect_changes`) both accept
  the new mode unchanged.

## 7. Error handling

| Case | Behavior |
|---|---|
| Base unresolvable / bad `--pr=<ref>` or `--pr-base <ref>` | Hard error with guidance to pass `--pr=<ref>` or `--pr-base <ref>` |
| No merge-base (unrelated histories) | Hard error |
| On base branch (merge-base == HEAD) | Warn to stderr, then show uncommitted-only (default) / empty (`--committed`) |
| Outside a git repo | Hard error (explicit mode — no plain-tree fallback) |
| `--committed` without `--pr`, `--pr` + another comparison flag, or both base override forms | clap/runtime usage error |

## 8. Affected components

| File | Change |
|---|---|
| `src/app.rs` | Add `pr: Option<Option<String>>`, `pr_base: Option<String>`, and `committed: bool` to `ViewArgs` with conflicts/requires. |
| `src/lib.rs` | Add `ComparisonMode::Pr { merge_base, committed }`; add `PrBase` + `resolve_pr_base`; add the `Pr` arm to `diff_files`; ensure `collect_changes`/`collect_all_files` handle it. |
| `src/main.rs` | Resolve base, emit on-base warning, slot `Pr` into mode selection / `explicit_mode` / `wants_plain_tree`; set `include_untracked = !committed`. |
| tests (`lib.rs` `#[cfg(test)]`, mirroring existing git2 temp-repo tests) | New behavioral tests (§9). |
| `README.md` | Add `--pr`, `--pr=<ref>`, `--pr-base <ref>`, path scopes, and `--committed` to the comparison-modes section. |

## 9. Test plan (behavioral — git2 temp repos)

- **Merge-base, not two-dot:** branch off `main`, add commits → `--pr` shows them. Then advance
  `main` with an unrelated change → that main-only change is **absent** from `--pr` output.
- **Endpoints:** default includes uncommitted + untracked; `--committed` excludes both and shows
  only branch commits.
- **Override:** `--pr=<ref>` and `--pr-base <ref>` resolve valid refs; bad explicit refs hard-error.
- **Path disambiguation:** `--pr src` scopes to path `src`; `--pr=main src` and
  `--pr --pr-base main src` combine explicit base and path scope.
- **Remote preference:** auto-detection prefers `origin/main`; explicit `--pr=main` resolves local
  `main` exactly first.
- **On base branch:** running `--pr` on `main` warns and shows uncommitted-only; `--pr --committed`
  on `main` is empty.
- **Auto-detect fallthrough:** repo with `master` (no `main`) resolves `master`.
- **Composition:** `--pr --all` lists the full tree with PR changes marked; `--pr --json` emits the
  PR change set.

## 10. Version & docs

- Feature → **minor version bump.** Note: `Cargo.toml` currently reads `0.1.0` even though the
  last milestone shipped as "v0.2" (the package version was never bumped). Reconcile during
  release; target the next minor (**v0.3.0**) for this feature.
- README comparison-modes section updated (§8).

## 11. Scope boundaries

**In scope:** the `--pr` / `--committed` CLI flags, `resolve_pr_base` + merge-base computation,
the `ComparisonMode::Pr` mode and `diff_files` arm, error handling, tests, README.

**Out of scope (separate follow-ups):**
- **TUI comparison-mode support** — the interactive TUI has *no* comparison-mode concept today
  (it only overlays working-tree status via `git::load_status`, gated on `-G`). Making `--pr`
  (or any comparison mode) work in the TUI is its own project; it should reuse `resolve_pr_base`
  and the merge-base helper from this work.
- Changing `--against` / `--range` semantics (left exactly as-is).
- Project tracking: this repo has no `PROJECTS.md`; work is tracked under `docs/`. Whether to
  introduce `PROJECTS.md` (per global conventions) or record the TUI follow-up under `docs/` is
  an open call for the spec reviewer — not part of the feature itself.
