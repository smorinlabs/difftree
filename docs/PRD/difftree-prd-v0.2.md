# difftree — Product Requirements Document (v0.2)

**Owner:** Steve Morin (smorinlabs)
**Status:** Scope locked — ready for acceptance-criteria pass
**Repo:** `smorinlabs/difftree` (Rust, MIT; clean fork seeded from `lstr`)

> v0.2 incorporates all interview decisions. Every prior open question is now **Resolved**. Remaining smaller details are marked **[DETAIL]** for the build phase.

---

## 1. Summary

difftree is a `tree`-style command-line tool that makes the *shape and impact of changes in a git repository* visible at a glance. Where `tree` shows a directory hierarchy and `git status` shows a flat list, difftree overlays the change onto the tree — so you instantly see the **blast radius** of your work: which directories are touched, how heavily, and where churn concentrates.

The hero experience is the **blast-radius view of staged changes**: a pruned tree of only the affected directories, with layered impact (changed-file count, line churn, and a heat indicator) rolled up per directory. When nothing is staged, it automatically falls back to the unstaged blast radius so the bare command is never empty-handed.

difftree is built as a **core library + thin CLI** so one engine powers pretty terminal output (v1), machine JSON for agents (v1), and an HTML view (roadmap).

## 2. Background & motivation

`git status` answers "what changed?" as a flat list and loses structure — you can't see whether a change is concentrated in one module or sprayed across ten, nor its relative weight. `tree` shows structure but is change-blind. difftree merges the two: structure + change + impact. The success bar is concrete: **the author reaches for difftree over `git status` in daily pre-commit review.**

## 3. Goals & non-goals

### Goals (v1)
- Make staged-change blast radius obvious in under a second on a normal repo.
- Beat `git status` for daily pre-commit review (pretty, glanceable).
- Ship JSON output good enough that agents/scripts can consume repo-change state reliably.
- Ship a clean core library so HTML is a later addition, not a rewrite.
- Honor git semantics users already know (staged/unstaged/both, gitignore).

### Non-goals (v1)
- Not an interactive TUI / file manager. **Strictly non-interactive in v1.**
- Not a `git` replacement; difftree reads repo state, never mutates it.
- Not an `ls` replacement; long-format, icons, themes are out.
- Not tuned for giant monorepos yet (performance is a later concern).

## 4. Target users

**Primary:** open-source CLI users broadly — developers who live in the terminal and use git daily. Implication: zero-config first run must impress; sensible defaults over flags; bare `difftree` already does something useful.

**Secondary (first-class via `--json` in v1):** AI coding agents needing structured repo-change state.

**Secondary (roadmap):** teams wanting a shared, glanceable branch blast-radius via the HTML view.

## 5. Product: vision vs v1

**Decision:** carve a tight v1 and publish the rest as a roadmap.

### 5.1 Full vision
Blast-radius view · full status-marked file tree · path scoping · all-files view with filters · ignored-files visualizer · committed-context overlays · line-change impact summaries · HTML web view · library API · machine/JSON output.

### 5.2 v1 slice (locked)
**In:**
- **Blast-radius view (hero)** — default = staged changes; pruned tree of touched dirs; layered impact per dir: changed-file count, +adds/−dels churn, heat indicator. Auto-fallback to **unstaged** blast radius when nothing is staged.
- **All comparison modes:** default staged; `--unstaged` (working tree vs index); `--all` (staged+unstaged); `--range <a>..<b>`; `--against <ref>`.
- **Full file-tree view with status marks** (staged / unstaged / both / untracked).
- **Plain `tree` mode + `tree` flag compatibility** (`--plain` / `--no-git`) — classic `tree` behavior on demand, even inside a git repo. difftree aims to be **mostly backwards-compatible with `tree`**: it honors `tree`'s common flags (e.g. `-d`, `-a`, `-f`, `-L`, `--dirsfirst`, `-P`/`-I`, `--prune`, `--noreport`) so existing muscle memory and scripts keep working. The **deliberate exceptions are difftree's new git-aware defaults** (bare `difftree` shows staged blast radius, not a full plain tree). See §6.8.
- **Path/subpath scoping** (`difftree src/`).
- **Pretty human output by default** (color + box-drawing), with **auto-plain when piped**, a **`--format`** override, and **`--json`** for agents.
- **`--no-color` / `NO_COLOR` / `--color=auto|always|never`.**
- **Gitignore handling, configurable, default hidden;** `--show-ignored` (inline, dimmed) and `--ignored` (dedicated visualizer).
- **Core library + thin CLI**, with a serializable change model from day one (so `--json` and future HTML share the core).
- **Distribution:** crates.io, GitHub prebuilt binaries, Homebrew, and Nix/Flox — all first-class for v1.

**Roadmap (post-v1):** HTML web view · all-files view with rich filters · committed-context overlays beyond scoping · large-repo performance · library API hardening/publishing as a stable crate.

## 6. Detailed behavior (v1)

### 6.1 Default invocation
- `difftree` inside a git repo → **blast-radius view of staged changes**, scoped to repo root.
- **No staged changes** → automatically show the **unstaged** blast radius (clearly labeled as unstaged so the mode switch is obvious). **[DETAIL]** label/heading wording.
- Outside a git repo → automatically behaves as **plain `tree` mode** (same as `--plain`), noting git features are unavailable. **[DETAIL]** confirm exact messaging.

### 6.2 Blast-radius view
- Includes only directories containing changes for the active comparison; prunes the rest.
- Each directory rolls up from descendants: **count** of changed files, **+adds / −dels**, and **heat**.
- **Heat encoding:** color + badge + bar **all shown by default**, with each individually toggleable (e.g. `--heat=color,bar,badge` or per-flag). **[DETAIL]** flag grammar for selecting heat components.
- Leaf files show their own status mark and per-file churn.
- **Staged line definition:** index vs HEAD (`git diff --cached` semantics); per-file stats from per-path diff. Other modes use their natural diff (`--unstaged` = working tree vs index, etc.).

### 6.3 Status model & marks
- Distinguish **staged**, **unstaged**, **both** (staged then further edited), and **untracked**.
- **Untracked files are included and clearly marked as untracked** (even in the staged view, labeled distinctly so they're never confused with staged content).
- **Mark schemes — all supported, switchable:** git letters (`A/M/D/R`), symbols (`●` staged / `○` unstaged), and two-column git-style (`XY`). **Default = symbol + color.** **[DETAIL]** exact symbol set, the "both" marker, and the flag name (e.g. `--marks=symbol|letter|xy`).

### 6.4 Comparison modes (all in v1)
- Default: **staged**.
- `--unstaged`, `--all`, `--range <a>..<b>`, `--against <ref>`. **[DETAIL]** precedence rules when combined; resolve name collision between `--all` (comparison) and any all-files flag (rename the latter, e.g. `--every-file`).
  - **Resolved (2026-06-21):** `--all` (alias `--tree`) names the **all-files view**; the combined staged+unstaged **comparison** is `--uncommitted`; `--staged`/`--cached` is the explicit staged comparison. This inverts the earlier `--every-file` suggestion. See `docs/superpowers/specs/2026-06-21-view-and-comparison-flags-design.md`.

### 6.5 Views & flag surface (proposed; naming pass pending)
- `difftree` → blast-radius (staged, auto-fallback unstaged).
- `difftree --tree` → full status-marked file tree.
- `difftree --plain` (alias `--no-git`) → classic `tree` behavior: full hierarchy, no git overlay or status marks.
- `difftree <path>` → scope to subpath.
- `difftree --ignored` → ignored visualizer; `--show-ignored` → inline dimmed.
- Output: `--format <pretty|plain>`, `--json`, `--color`, `--no-color` (+ `NO_COLOR`), auto-plain on pipe.
- Tree controls: `--depth/-L`, heat/marks selectors.
- **[DETAIL]** full flag table + short forms in the acceptance-criteria pass.

### 6.6 Ignored files
- Default hidden. `--show-ignored` surfaces dimmed/marked inline; `--ignored` is a dedicated visualizer. Honors nested `.gitignore` + global excludes (via the `ignore` crate).

### 6.7 Output & rendering
- **Pretty by default** when attached to a TTY: box-drawing glyphs, colored marks, per-dir impact (count + churn + heat), and a summary footer (`N dirs touched · M files changed · +X −Y`).
- **Auto-plain when piped**; `--format` forces either way.
- **`--json`** emits the serializable change model for agents/scripts. **[DETAIL]** lock the JSON schema (tree nodes, per-node status, per-node/rolled-up counts and churn, comparison metadata) early, since it's a v1 contract.

### 6.8 `tree` compatibility

**Principle:** difftree is **mostly backwards-compatible with `tree`**. Flags that exist in `tree` keep their `tree` meaning, so existing commands and scripts mostly "just work." Divergence is allowed only where difftree's git-aware purpose requires it, and every divergence is intentional and documented.

- **Honored `tree` flags (v1 target):** `-a` (all), `-d` (dirs only), `-f` (full paths), `-L <n>` (max depth), `-P <pat>` / `-I <pat>` (include/exclude patterns), `--prune`, `--dirsfirst`, `--noreport`, `-n`/`-C` (color off/on), `--filelimit`. **[DETAIL]** finalize the exact supported set in the flag-table pass; aim for the high-use flags, not 100% parity.
- **Deliberate exceptions (new defaults):**
  - Bare `difftree` shows the **staged blast-radius view**, not a full plain tree. (`--plain` / `--no-git` restores classic behavior.)
  - Default output carries git **status marks and impact annotations** that `tree` has no concept of.
  - Color/`NO_COLOR` handling follows difftree's modern model (`--color=auto|always|never`) while still accepting `tree`'s `-n`/`-C`.
- **Conflict rule:** where a `tree` flag and a difftree flag would collide in meaning, the `tree` flag keeps its original semantics and the difftree feature gets a new, non-colliding name (e.g. the all-files comparison flag is **not** `-a`). **[DETAIL]** audit collisions during the flag-table pass.
- **Out of scope for parity:** `tree`'s HTML (`-H`) and JSON (`-J`) output formats are replaced by difftree's own `--json` (v1) and HTML view (roadmap) rather than reproducing `tree`'s exact schemas.

## 7. Success metrics

**Primary:** author uses difftree instead of `git status` for daily pre-commit review. Operationalized as a 2-week dogfood with honest self-report ("did I reach for it without thinking?").

**Supporting (directional, post-launch, not the bar):** faster time-to-answer for "what's my blast radius?"; agent-parse reliability of `--json`; install/adoption signals.

## 8. Architecture & constraints

- **Core library + thin CLI** is a hard requirement. The engine exposes a traversal + change model **independent of rendering**, with a `Renderer` trait; terminal (v1), JSON (v1), and HTML (roadmap) all consume the same core. The model is **serializable from day one** to back `--json`.
- Seeded as a clean fork of `lstr` (MIT) — already carries `ignore`-based gitignore handling, a git-status overlay, and color-mode support — **pending the renderer-seam validation spike** (can lstr's terminal output route through a `Renderer` trait without touching traversal/git?). If it fails, fall back to greenfield on `ignore` + `git2` + `anstyle` + `clap`.
- **Git backend:** `git2` for v1 (mature staged/unstaged + diff stats); `gix` behind a trait as a future pure-Rust swap.
- **Distribution (v1, all first-class):** crates.io · GitHub prebuilt binaries · Homebrew · Nix/Flox. **[DETAIL]** release automation (CI cross-builds, signing, tap + flake) is its own work item.

## 9. Remaining build-phase details
1. Heading/wording for the staged→unstaged auto-fallback.
2. Heat-component flag grammar.
3. Status symbol set + "both" marker + `--marks` flag.
4. Comparison-flag precedence + **`tree`-flag compatibility audit**: confirm the honored `tree` flag set and resolve every collision between `tree` semantics and difftree flags (e.g. `--all` comparison vs `tree`'s `-a`; rename difftree's all-files flag, e.g. `--every-file`).
5. Full flag table with short forms.
6. JSON schema (v1 contract — lock early).
7. Release automation across the four channels.

## 10. Out of scope (explicit)
Interactive TUI; mutating git state; `ls`-style long listings, icons, themes; large-monorepo performance tuning; HTML output (roadmap); committed-context overlays beyond path scoping (roadmap).

---

### Next step
Lock the **JSON schema** and the **flag table** (items 5–6) — they're the two v1 contracts everything else builds on. I can draft both next, plus per-view acceptance criteria, to turn this into a build-ready v0.3.
