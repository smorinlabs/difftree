# difftree — All-files view + comparison-mode flag redesign

**Date:** 2026-06-21
**Status:** Design approved — ready for implementation plan
**Branch:** `codex/implement-prd-from-difftree-goal-v0.2`
**Related:** `docs/PRD/difftree-prd-v0.2.md` (§6.1, §6.4, §6.5, §6.8), `docs/specs/difftree-decisions-v0.2.md`

---

## 1. Problem

Two issues, discovered while planning the `--all` rename:

1. **Flag-name collision (PRD §6.4 / §6.8, unresolved).** `--all` currently names the
   *combined staged+unstaged comparison mode*. But "all" reads more naturally as "all files,"
   and the PRD explicitly flagged this collision as needing resolution.

2. **`--tree` is broken — the all-files view does not actually exist.** `--tree` claims to
   render the "full status-marked file tree," but `lib.rs::build_tree` only constructs nodes
   from *changed* files plus their ancestor directories. There is no filesystem walk, so
   unchanged files never appear.

   Reproduced (one staged change to `src/committed.rs`, unchanged `docs/readme.md` on disk):

   ```
   --tree   →  shows ONLY src/committed.rs        (docs/readme.md missing)   ❌
   --all    →  identical to --tree (changes-only)                            ❌
   --plain  →  shows BOTH files, but with no git marks                       ✓ (no overlay)
   ```

   So the true "every file, with change marks overlaid" view is unimplemented; `--plain` is
   the only thing that walks the whole tree and it has no git overlay.

## 2. Core idea: separate *view* from *comparison*

The current code conflates two orthogonal concepts. We make them explicit:

- **View** — *which nodes to show:*
  - `blast-radius` — pruned to touched directories (the default, the hero view).
  - `all-files` — the complete directory tree, with change marks overlaid on changed files.
- **Comparison** — *what counts as "changed":*
  - `staged`, `unstaged`, `uncommitted`, `against <ref>`, `range <a>..<b>`.

The view flag and the comparison flag compose. `--all` selects the all-files **view**;
the comparison flags select **what gets marked**. A view is never a comparison and vice-versa.

## 3. Flag surface (target)

| Flag | Alias | Meaning |
|---|---|---|
| `difftree` (bare) | — | blast-radius view, comparison = staged → auto-fallback to unstaged *(unchanged)* |
| `--all` | `--tree` | **all-files view** — every file; `●`/`○`/`◐` marks on changed files, `Clean` (blank) otherwise |
| `--staged` | `--cached` | comparison: index vs HEAD *(new explicit flag; staged remains the bare default)* |
| `--unstaged` | — | comparison: working tree vs index *(unchanged)* |
| `--uncommitted` | — | comparison: staged + unstaged + untracked vs HEAD *(renamed from today's `--all`)* |
| `--against <ref>` | — | comparison: working tree+index vs `<ref>` *(unchanged)* |
| `--range <a>..<b>` | — | comparison: tree `<a>` vs tree `<b>` *(unchanged)* |

Default comparison for `--all`: same resolution as bare `difftree` (staged → unstaged
fallback) unless an explicit comparison flag is given. `--all` composes with any comparison
flag, e.g. `difftree --all --uncommitted`.

The auto-fallback banner (`ChangeTree.fallback`) currently hard-codes "…showing unstaged
blast radius". Generalize the wording so it is accurate under either view (e.g. "No staged
changes — showing unstaged changes"); the renderer prepends it for both blast-radius and
all-files.

### 3.1 `--uncommitted` is a secondary mode (not the default)
Decision: bare `difftree` keeps the PRD §6.1 default (staged, fallback to unstaged).
`--uncommitted` is an explicit opt-in. Rationale: `--uncommitted` ≈ `git diff HEAD` (+ untracked)
≈ `git status` scope, and is the right view for reviewing a *partially staged* working tree
(complete footprint, with `●/○/◐` preserving the staged-vs-unstaged distinction in one tree),
but the PRD deliberately chose staged-first as the hero, and we honor that.

## 4. Collision & `tree`-compatibility (PRD §6.8)

- `-a` / `--show-all` (show hidden/dotfiles) **keeps its `tree` meaning** — untouched.
- The all-files flag is `--all`, **long-only, with no `-a` short form**, honoring the PRD §6.8
  rule that the all-files flag is *not* `-a`. The two compose: `--all -a` = every file
  including hidden.
- **This inverts PRD §6.4's tentative naming** (which had penciled in `--every-file` for the
  all-files flag and kept `--all` for the comparison). The decisions doc
  (`docs/specs/difftree-decisions-v0.2.md`) and PRD §6.4 are updated to record this inversion.

## 5. Implementation (Approach A — unified model)

One engine powers blast-radius, all-files, and `--json`.

- New `lib.rs` collection entry point for the all-files view:
  1. Walk the scoped path with the `ignore` crate, honoring `-a`, `-g`/`--gitignore`,
     `-L`/`--depth`, `-d`/`--dirs-only`, and the sort options (reuse `sort.rs`).
  2. Build a **complete** `TreeNode` tree (every directory and file).
  3. Overlay each file's `status` + `churn` from the existing `path → FileChange` map
     produced by the selected comparison. Unchanged files → `ChangeStatus::Clean`.
- Rollups keep their current meaning (directories roll up *changed*-file counts and churn from
  descendants), now displayed within the full tree rather than a pruned one.
- Blast-radius collection and `JsonRenderer`/`TerminalRenderer` continue to consume the same
  `ChangeTree` model — no second rendering path.

## 6. JSON contract impact (v1, still unreleased)

- Rename `ComparisonMode::All` → `ComparisonMode::Uncommitted`
  ⇒ `--json` emits `"comparison": "Uncommitted"`.
- Add a `"view": "blast-radius" | "all-files"` field to `ChangeTree` so consumers can tell
  which view they received.
- `schema_version` stays `"difftree.v1"` (still pre-release; no compat guarantees broken).

## 7. Affected components

| File | Change |
|---|---|
| `src/app.rs` | Repurpose `all` → all-files view bool; add `--tree` as its alias. Add `--uncommitted` and `--staged` (alias `--cached`). |
| `src/main.rs` | Resolve *view* (blast-radius vs all-files) and *comparison* (staged/unstaged/uncommitted/against/range) independently; dispatch all-files to the new collection fn. |
| `src/lib.rs` | Rename `ComparisonMode::All`→`Uncommitted`; add all-files walk+overlay collection; add `view` field to `ChangeTree`; build the full tree. |
| `tests/cli.rs` | New: all-files view shows unchanged files (`Clean`) alongside changed (`●`); `--uncommitted`; `--staged`; JSON `view` field + renamed comparison value. |
| `docs/PRD/difftree-prd-v0.2.md`, `docs/specs/difftree-decisions-v0.2.md` | Record the `--all`/`--uncommitted` naming inversion. |

## 8. Test plan (behavioral)

- **all-files view**: repo with one changed file and one unchanged file → `--all` lists *both*;
  changed file carries `●`/`○`/`◐`, unchanged carries no mark; `--all -a` additionally lists
  hidden files; `--all -L 1` respects depth.
- **`--uncommitted`**: partially staged repo (one staged + one unstaged file) → both appear with
  distinct marks; summary footer counts both.
- **`--staged` / `--cached`**: equals the bare-default staged set; `--cached` is an exact alias.
- **comparison default for `--all`**: `--all` with no comparison flag marks the staged
  (fallback-unstaged) set; `--all --unstaged` marks the unstaged set.
- **JSON**: `--json` emits `"view"` and the renamed `"comparison": "Uncommitted"` value;
  all-files `--all --json` includes unchanged-file nodes with `"status": "Clean"`.

## 9. Scope boundaries

**In scope:** the view/comparison split, flag renames (`--uncommitted`, `--staged`/`--cached`),
the real all-files walk+overlay (Approach A), the `view` JSON field, tests, and the
PRD/decisions-doc naming update.

**Out of scope (separate follow-ups, unchanged by this work):**
- Line-churn counts — still render `+0 −0` (`Churn::default()`; per-file diff stats not computed).
- `--heat` rendering (flag parsed, not yet visualized).
- `--show-ignored` / `--ignored` visualizer wiring.
- `tree`-compat flags `-P` / `-I` / `--prune` / `--noreport` / `--filelimit` wiring.
