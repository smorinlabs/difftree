# difftree — Goal: Implement the v0.2 PRD in full (TDD)

**Owner:** Steve Morin (smorinlabs)
**Status:** Ready to execute
**Implements:** [`docs/PRD/difftree-prd-v0.2.md`](../PRD/difftree-prd-v0.2.md) — the source of truth
**Work branch:** `claude/difftree-v0.2-prd-7resks`
**Naming convention:** `docs/goals/difftree-goal-v<version>.md` (kebab-case, version is the sort key; each goal tracks the PRD version it implements)

---

## 0. How to use this document

**This file is a prompt.** Hand it to a coding agent (or run it as a goal/command) and it will drive the *complete, test-driven* implementation of difftree v1 as specified by PRD v0.2. Everything below is addressed to the implementing agent.

> **Agent:** Your job is to make the v0.2 PRD real — a working `difftree` binary plus the core library behind it — using strict test-driven development. The PRD at `docs/PRD/difftree-prd-v0.2.md` is the contract. Where it marks a decision `[DETAIL]`, **you decide, you justify, you document, you test.** Do not invent features the PRD does not call for. Do not cut features the PRD locks in.

---

## 1. Mission

Deliver the **v1 slice locked in PRD §5.2** end to end:

- The **hero blast-radius view** of staged changes, with auto-fallback to unstaged.
- **All comparison modes** (`--unstaged`, `--all`, `--range`, `--against`).
- **Full status-marked file tree** (`--tree`).
- **Plain `tree` mode** + `tree` flag compatibility (`--plain` / `--no-git`).
- **Path/subpath scoping.**
- **Pretty TTY output** (auto-plain when piped), **`--format`**, **`--json`**, color controls.
- **Ignored-file handling** (`--show-ignored`, `--ignored`).
- **Core library + thin CLI** with a **serializable change model** from day one.

Success bar (PRD §2, §7): the tool is good enough that the author reaches for `difftree` over `git status` in daily pre-commit review.

## 2. Scope

**In scope (this goal):** the full v1 feature set above, the core library + thin CLI split, and a complete automated test suite.

**Deferred (NOT this goal):** distribution / release automation across crates.io · GitHub prebuilt binaries · Homebrew · Nix/Flox (PRD §9 item 7). Keep the code release-ready (clean `Cargo.toml`, versioning, `--version`), but do not build the release pipelines here.

**Out of scope (PRD §10):** interactive TUI, mutating git state, `ls`-style long listings / icons / themes, large-monorepo perf tuning, HTML output, committed-context overlays beyond path scoping.

> The seed is a fork of `lstr`, which ships a TUI (`src/tui.rs`). v1 is **strictly non-interactive** (PRD §3). Either gate the TUI out of the v1 surface or remove it; it must not be reachable from the documented v1 CLI. Decide and document.

## 3. Phase 0 — Lock the contracts (do this first)

PRD §9 lists build-phase `[DETAIL]`s. Resolve each **before** the code that depends on it, and record every decision in a new **`docs/specs/`** doc (e.g. `docs/specs/difftree-decisions-v0.2.md`) plus the user-facing `README`/`--help`. Each decision gets a test that pins it.

Lock, at minimum:

1. **Auto-fallback wording** (§6.1) — the heading/label shown when staged is empty and difftree falls back to unstaged. Must be unmistakable.
2. **Heat-component flag grammar** (§6.2) — how `color` / `bar` / `badge` are selected and toggled (e.g. `--heat=color,bar,badge` and/or per-component flags).
3. **Status marks** (§6.3) — exact symbol set, the **"both"** (staged-then-edited) marker, the untracked marker, and the `--marks=symbol|letter|xy` flag. Default = symbol + color.
4. **Comparison-flag precedence** (§6.4) + the **`tree`-flag collision audit** (§6.8): confirm the honored `tree` flag set and rename every difftree flag that would collide with `tree` semantics (e.g. the all-files flag is **not** `-a` — use `--every-file` or similar).
5. **Full flag table with short forms** (§6.5) — the complete CLI surface, committed as a table.
6. **JSON schema** (§6.7) — the v1 contract: tree nodes, per-node status, per-node and rolled-up counts + churn, and comparison metadata. Version the schema. **Lock this early; everything serializes through it.**

Phase 0 is done when the decisions doc exists, the flag table and JSON schema are written down, and each is referenced by the tests that enforce it.

## 4. Architecture (hard constraints — PRD §8)

- **Core library + thin CLI is mandatory.** Split into a `difftree` library crate (traversal + change model + rendering traits) and a thin binary that only parses args and calls the library. The model must be **serializable from day one** (serde) so `--json` and any future HTML view share one core.
- **`Renderer` trait seam.** Terminal (v1) and JSON (v1) are two implementations of one `Renderer`; HTML is a roadmap third. **Run the renderer-seam validation spike first** (PRD §8): can the seeded lstr terminal output route through a `Renderer` without touching traversal/git? If the spike fails, fall back to greenfield on `ignore` + `git2` + `anstyle` + `clap` — and document that you did.
- **Git backend behind a trait.** Use `git2` for v1; isolate it behind a trait so `gix` can swap in later. difftree **reads** repo state and **never mutates** it.
- **Gitignore** via the `ignore` crate; honor nested `.gitignore` + global excludes.
- Preserve **lstr attribution** (`NOTICE`, README credits, MIT) through all refactors.

## 5. TDD methodology (non-negotiable)

Work in strict **red → green → refactor** cycles:

1. **Red.** Write a failing test that encodes the next behavior (unit test for library logic; `tests/cli.rs`-style integration test for CLI behavior; golden/snapshot tests for rendered trees and JSON). Run it; watch it fail for the *right* reason.
2. **Green.** Write the minimum code to pass. No gold-plating.
3. **Refactor.** Clean up with tests green.

Rules:

- **No production code without a failing test first.** Every feature and every resolved `[DETAIL]` ships with tests that pin the decision.
- **Test fixtures:** build throwaway git repos in temp dirs (init, stage, edit, add untracked, etc.) to exercise real `git2` behavior. Reuse `examples/sample-directory/` for plain-tree cases.
- **Golden tests** for rendered output (pretty + plain) and for the **JSON schema** (validate shape and key fields; treat the JSON as a stable contract — a schema change must break a test).
- **Keep CI green at every phase boundary.** Use the existing `justfile` recipes (`build`, `format`, `lint`, `typecheck`, `test`). A phase is not "done" until `just test` (or equivalent) passes, `clippy` is clean, and `rustfmt` is applied.
- **Determinism:** sort entries deterministically; never let test output depend on filesystem ordering, locale, or color autodetection. Force color/format explicitly in tests.

## 6. Phased plan (each phase: tests first, then code, then CI green)

For **every** phase, the acceptance criteria below are the spec. Translate each into failing tests before implementing.

### Phase 1 — Library/CLI split + model + Renderer seam
- Library crate exposes a serializable change/tree model and a `Renderer` trait; binary is thin.
- Renderer-seam spike validated (or documented fallback).
- **AC:** model round-trips through serde; a trivial renderer prints a tree via the trait; binary delegates to the library; existing tests still pass.

### Phase 2 — Git backend + status model
- `git2` behind a trait. Compute **staged / unstaged / both / untracked** per path, and per-file **+adds / −dels** (staged = index vs HEAD, `git diff --cached` semantics; unstaged = working tree vs index).
- **AC:** against temp fixture repos, each status class and churn count is correct, including renames (`R`) and untracked files surfaced distinctly.

### Phase 3 — Blast-radius view (HERO)
- Pruned tree of only changed dirs; per-dir rollup of count + churn + heat; leaf files show own mark + churn; summary footer (`N dirs touched · M files changed · +X −Y`).
- Default = staged; **auto-fallback to unstaged when nothing staged**, clearly labeled (Phase 0 wording).
- Heat = color + badge + bar by default, each toggleable (Phase 0 grammar).
- **AC:** bare `difftree` in a repo with staged changes renders the pruned, rolled-up tree; with nothing staged it renders the labeled unstaged fallback; rollups equal the sum of descendants; golden tests for pretty output.

### Phase 4 — Comparison modes
- `--unstaged`, `--all`, `--range <a>..<b>`, `--against <ref>`, with documented precedence (Phase 0).
- **AC:** each mode selects the correct diff; precedence resolves combined flags as documented; invalid refs/ranges error cleanly.

### Phase 5 — Full status-marked tree (`--tree`) + mark schemes
- `--tree` shows the full hierarchy with status marks; `--marks=symbol|letter|xy` switches schemes; default symbol + color.
- **AC:** each mark scheme renders the documented glyphs incl. the "both" and untracked markers; golden tests per scheme.

### Phase 6 — Plain tree mode + `tree` compatibility
- `--plain` / `--no-git` = classic `tree` (no git overlay). Honor the high-use `tree` flags from §6.8: `-a`, `-d`, `-f`, `-L <n>`, `-P`/`-I`, `--prune`, `--dirsfirst`, `--noreport`, `-n`/`-C`, `--filelimit`.
- Outside a git repo, bare `difftree` behaves as `--plain` with a note that git features are unavailable.
- **AC:** for the honored flags, output matches `tree`'s semantics on `examples/sample-directory/`; every documented collision is resolved (difftree feature renamed, `tree` flag keeps its meaning).

### Phase 7 — Path/subpath scoping
- `difftree <path>` scopes any view to a subpath.
- **AC:** scoping limits traversal and rollups to the subtree; works with all views and comparison modes.

### Phase 8 — Ignored files
- Default hidden; `--show-ignored` inline dimmed/marked; `--ignored` dedicated visualizer. Honor nested `.gitignore` + global excludes.
- **AC:** ignored entries are hidden by default, dimmed inline with `--show-ignored`, and listed by `--ignored`; nested ignore rules respected.

### Phase 9 — Output, color, piping
- Pretty by default on a TTY (box-drawing, colored marks, per-dir impact, footer); **auto-plain when piped**; `--format <pretty|plain>` forces; `--color=auto|always|never`, `--no-color`, `NO_COLOR`, plus `tree`'s `-n`/`-C`.
- **AC:** piping yields plain output; `--format`/color flags and `NO_COLOR` behave per the documented precedence; tests force every mode explicitly.

### Phase 10 — JSON output (`--json`)
- Emit the locked Phase-0 schema: nodes, per-node status, per-node + rolled-up counts/churn, comparison metadata, schema version.
- **AC:** `--json` validates against the locked schema for every comparison mode and view; an agent can reconstruct the tree and totals from it; schema changes break a golden test.

### Phase 11 — Docs, help, final pass
- README usage for every view/flag; accurate `--help`; CHANGELOG updated; `--version` works.
- **AC:** README and `--help` match the implemented flag table exactly; full suite + clippy + fmt green; manual dogfood of the hero view on this very repo looks right.

## 7. Definition of done

- [ ] Every PRD §5.2 v1 feature implemented and covered by tests.
- [ ] Every PRD §9 `[DETAIL]` (items 1–6) resolved, documented in `docs/specs/`, and pinned by a test.
- [ ] Core library + thin CLI split; model serializable; `Renderer` trait drives terminal + JSON.
- [ ] git state never mutated; lstr attribution intact.
- [ ] `just build`, `just test`, `just lint`, `just format` all clean; CI green.
- [ ] JSON schema documented and golden-tested as a v1 contract.
- [ ] README + `--help` accurate; CHANGELOG updated.
- [ ] Release automation explicitly deferred (noted, not built).

## 8. Working agreement

- Develop on `claude/difftree-v0.2-prd-7resks`. Commit per phase with clear messages; keep each commit green. Push when phases complete. **Do not open a PR unless explicitly asked.**
- Prefer the PRD's locked defaults over new flags; when the PRD is silent, choose the simplest behavior that serves "beat `git status` for pre-commit review," and document it.
- If a decision is genuinely ambiguous or architecturally significant beyond a documented `[DETAIL]`, stop and ask rather than guess.
