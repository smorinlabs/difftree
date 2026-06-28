# difftree — Phase 1: semantic color for comparison views

**Date:** 2026-06-27
**Status:** Design approved — ready for implementation plan
**Branch:** `feat/pr-diff-shortcut`
**Related:** `docs/specs/difftree-decisions-v0.2.md` (§ Heat flag grammar), `docs/PRD/difftree-prd-v0.2.md` (§6.4 heat encoding), `docs/superpowers/specs/2026-06-27-pr-diff-shortcut-design.md`

---

## 1. Problem

The comparison-mode renderer (`TerminalRenderer` in `src/lib.rs`) applies **no color at all** —
status marks (`●`/`×`/…), churn (`+N −M`), and filenames are plain strings. Every comparison
mode (`--pr`, `--staged`, `--unstaged`, `--uncommitted`, `--all`, `--range`, `--against`) is
therefore monochrome, while the plain-tree view (`src/view.rs`) is fully colored (status char by
git state, filenames via LsColors). `main.rs` already wires `--color`/`NO_COLOR`/`--force-color`
into `colored`'s global override, but `TerminalRenderer` never calls `colored`, so those flags have
no effect on it. This is the unimplemented `color` half of the v0.2 `--heat=color,bar,badge`
grammar ("parsed, not yet visualized").

## 2. Scope: Phase 1 = semantic color only

The `--heat` grammar has three components; this spec delivers the first:

- **Phase 1 (this spec): `color`** — semantic color on marks, churn, and filenames.
- **Phase 2 (separate spec, deferred): `bar` + `badge`** — heat-magnitude visualization, where
  heat = **changed-file count** for directories and **total churn (added+deleted)** for files.

Phase 1 does **not** wire `--heat` parsing. Semantic color applies whenever color is enabled,
gated solely by the existing controls (`--color`/`NO_COLOR`/`--force-color`, plus `colored`'s
auto-off-when-piped TTY detection). `--json` and the plain-tree view are unchanged except for the
shared-helper extraction in §5.

## 3. What gets colored

Applied in `TerminalRenderer` for every comparison mode (shared renderer ⇒ one change covers all):

1. **Status marks** — colored by `ChangeStatus`:

   | Status | Color | Status | Color |
   |---|---|---|---|
   | Staged | Green | Renamed | Blue |
   | Unstaged | Yellow | Deleted | Red |
   | Both | Cyan | Ignored | BrightBlack (dim) |
   | Untracked | Magenta | Clean | none (default) |

   (mirrors the `view.rs` palette; `Clean` marks stay uncolored.)

2. **Churn** — `+N` green and `−M` red, at all three sites: per-file metric (` +N −M`),
   per-directory rollup (` (N files, +X −Y)`), and the summary footer
   (`… files changed · +X −Y`). Coloring is applied regardless of magnitude (`+0` green / `−0` red).

3. **Filenames** — styled via LsColors (foreground color + bold/italic/underline), matching the
   plain-tree view.

## 4. Edge cases: renamed & deleted

- **Deleted** (`×`, Red mark; churn `−M` red, `+0`): the path may not exist on disk (working-tree
  modes *and* tree-to-tree modes like `--pr --committed`/`--range`). `ls_colors.style_for_path(...)`
  returns `None` for a missing path → `.unwrap_or_default()` yields a plain style. **No panic, no
  error**; the red `×` conveys deletion.
- **Renamed** (`↻`, Blue mark): the model already keys a rename to its **new** path
  (`new_file().path()`), so it renders under the new name and its LsColors lookup works normally.
  The old path is not shown — existing behavior, unchanged here.

## 5. Shared LsColors helper (DRY)

`view.rs:195–231` converts a `lscolors::Style` (foreground `lscolors::Color` + `font_style`
bold/italic/underline) into a `colored`-styled `String`. Extract this into one reusable function
(e.g. `pub fn style_name(name: &str, style: &lscolors::Style) -> String` in `src/utils.rs`) and
call it from **both** `view.rs` and `TerminalRenderer`. Eliminates duplication and keeps the two
views' filename styling identical. The full `LsColor` match arm (Black…RGB) moves into the helper
verbatim.

## 6. Renderer interface change

`TerminalRenderer` currently sees only the in-memory `ChangeTree` (names + relative path strings),
which is insufficient for accurate LsColors lookups (type + extension detection benefits from a
real path). Changes:

- `TerminalRenderer` gains two inputs: `ls_colors: &LsColors` and the **scope root path**
  (`PathBuf`). For each file node it computes `root.join(&node.path)` and calls
  `ls_colors.style_for_path(abs)` for full type+extension fidelity (directories use the directory
  style). Marks and churn need no path.
- `main.rs` already constructs `LsColors::from_env()` and holds `view_args.path`; it passes both
  into the renderer at construction. (`TerminalRenderer { marks, format, ls_colors, root }` or an
  equivalent constructor.)

Gating note: marks and churn use `colored`'s `.color()`/`.green()`/`.red()`, which respect the
global override `main.rs` already sets. Filename styling is applied **through** `colored` (the
helper builds a `ColoredString`), so it honors the same override and TTY detection — color off ⇒
plain output for all three element types uniformly.

## 7. Affected components

| File | Change |
|---|---|
| `src/utils.rs` | Add `style_name(name, &lscolors::Style) -> String` (extracted LsColors→colored conversion). |
| `src/view.rs` | Replace the inline `195–231` conversion with a call to `utils::style_name`. |
| `src/lib.rs` | `TerminalRenderer`: add `ls_colors`/`root`; color marks (`mark_color`), churn (`+`green/`−`red helper), and filenames (`utils::style_name`); keep `JsonRenderer` untouched. |
| `src/main.rs` | Pass `ls_colors` and the scope root into `TerminalRenderer`. |
| tests (`src/lib.rs` `#[cfg(test)]`) | Color-on (ANSI present) and color-off (plain) assertions over a hand-built `ChangeTree`. |
| `README.md` | Note that comparison views are colored (marks/churn/filenames) when color is enabled. |

## 8. Test plan

Color is gated and TTY-sensitive, so tests force the state explicitly with
`colored::control::set_override(true|false)` (set and reset within each test; avoid cross-test
bleed):

- **marks**: a `Staged` file's rendered line contains the green ANSI sequence; a `Deleted` file's
  contains red.
- **churn**: `+N` is wrapped in green and `−M` in red — verified on a per-file line and on the
  summary footer.
- **filenames**: a node whose `LS_COLORS`/style yields a foreground color renders the filename with
  that ANSI color (drive via a constructed `LsColors` so the test is deterministic).
- **gate**: with `set_override(false)`, the same tree renders with **no** ANSI escapes (plain),
  proving `--color=never`/`NO_COLOR`/pipe behavior.
- **deleted path safety**: a `Deleted` node whose path does not exist on disk renders without panic
  (plain/extension filename, red `×`).

## 9. Scope boundaries

**In scope:** semantic color (marks/churn/filenames) in `TerminalRenderer` for all comparison
modes; the shared `style_name` helper + `view.rs` refactor to use it; renderer interface change to
thread `ls_colors`/`root`; tests; a README note.

**Out of scope (separate follow-ups):**
- `bar` + `badge` heat components and `--heat` parsing (Phase 2).
- Any change to `--json` output (color is presentation-only).
- Plain-tree view behavior beyond swapping in the shared helper.
- Strikethrough/dim styling for deleted filenames (the red `×` mark suffices).
