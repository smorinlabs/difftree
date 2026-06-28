# Comparison-View Semantic Color (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add semantic color (status marks, churn, filenames) to the shared `TerminalRenderer` so every comparison mode (`--pr`, `--staged`, `--unstaged`, `--uncommitted`, `--all`, `--range`, `--against`) renders in color when color is enabled.

**Architecture:** Extract the existing LsColors→`colored` filename-styling block from `view.rs` into a shared `utils::style_name`, used by both views (DRY). Then make `TerminalRenderer` color marks (by `ChangeStatus`), churn (`+`green/`−`red), and filenames (via `utils::style_name`), threading the already-built `LsColors` and the scope root path in from `main.rs`. All coloring goes through the `colored` crate, which respects the global enable/disable override `main.rs` already sets from `--color`/`NO_COLOR`/`--force-color`.

**Tech Stack:** Rust, `colored` (global override + `Colorize`), `lscolors` (filename styles), `git2` model; tests use `colored::control::set_override` + a crate-wide test mutex; `tempfile` for path-based tests.

## Global Constraints

- No new crate dependencies (use existing `colored`, `lscolors`, `tempfile`).
- Color is presentation-only: **no change to `JsonRenderer`/`--json` output** or the serializable model.
- Gating: coloring is applied through `colored`, honoring the global override `main.rs` sets (`--color`/`NO_COLOR`/`--force-color`) and `colored`'s auto-off-when-piped TTY detection. `--heat` is NOT wired in this phase.
- Palette (marks): Staged→Green, Unstaged→Yellow, Both→Cyan, Untracked→Magenta, Renamed→Blue, Deleted→Red, Ignored→BrightBlack, Clean→none.
- Churn: `+N` green, `−M` red (the `−` is U+2212 MINUS SIGN, matching existing output) at all three sites: per-file, per-dir rollup, summary footer.
- Filenames styled via LsColors must match the plain-tree view exactly (shared helper).
- clippy `-D warnings` clean; tests pristine. TDD (failing test first). Conventional Commits.
- Tests that mutate the global `colored` override MUST serialize via the shared `crate::test_color::guard()` lock to avoid parallel flakiness.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src/lib.rs` | Model + renderers (LIBRARY crate `difftree`) | Add `pub fn style_name(...)` (exported as `difftree::style_name`); add `#[cfg(test)] pub(crate) mod test_color` lock; color `TerminalRenderer` (marks/churn/filenames); add lifetime + `ls_colors`/`root` fields. NOTE: `utils.rs`/`view.rs` are in the BINARY crate (`main.rs` `mod`s); the library cannot call into them, so the shared helper lives in the library and the binary reaches it via `difftree::`. |
| `src/view.rs` | Plain-tree view | Replace the inline LsColors block with a `utils::style_name` call. |
| `src/main.rs` | CLI dispatch | Pass `ls_colors` + canonicalized scope root into `TerminalRenderer`. |
| `README.md` | Docs | Note comparison views are colored when color is enabled. |

---

## Task 1: Shared `utils::style_name` helper + `view.rs` refactor + test lock

Extract the LsColors→`colored` filename styling into one reusable function and route `view.rs` through it. Also add the crate-wide test mutex used by all color tests.

**Files:**
- Modify: `src/lib.rs` (add `style_name` + its tests + `test_color` module — all in the LIBRARY crate)
- Modify: `src/view.rs:195-231` (replace block with `difftree::style_name` call)

**Interfaces:**
- Produces: `pub fn style_name(name: &str, style: &lscolors::Style) -> String` (in `lib.rs`, exported as `difftree::style_name`). Consumed by `view.rs` (binary, via `difftree::style_name`) and `TerminalRenderer` (Task 2, same library crate via `crate::style_name`).
- Produces: `#[cfg(test)] pub(crate) mod test_color { pub fn guard() -> std::sync::MutexGuard<'static, ()> }` (in `lib.rs`). Consumed by color tests in Tasks 1 & 2.

- [ ] **Step 1: Add the crate-wide test-color lock to `src/lib.rs`**

Append to the end of `src/lib.rs`:

```rust
#[cfg(test)]
pub(crate) mod test_color {
    use std::sync::{Mutex, MutexGuard};
    static LOCK: Mutex<()> = Mutex::new(());
    /// Serializes tests that mutate the global `colored` override (a process-wide
    /// atomic), preventing parallel tests from clobbering each other's setting.
    pub fn guard() -> MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }
}
```

- [ ] **Step 2: Write the failing `style_name` tests**

Append to the `#[cfg(test)] mod tests` block in `src/utils.rs`:

```rust
    #[test]
    fn style_name_applies_foreground_when_color_on() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let style = lscolors::Style {
            foreground: Some(lscolors::Color::Green),
            ..Default::default()
        };
        let out = style_name("file.rs", &style);
        assert!(out.contains("\x1b[32m"), "green ANSI present when color on: {out:?}");
        assert!(out.contains("file.rs"));
        colored::control::unset_override();
    }

    #[test]
    fn style_name_plain_when_color_off() {
        let _c = crate::test_color::guard();
        colored::control::set_override(false);
        let style = lscolors::Style {
            foreground: Some(lscolors::Color::Green),
            ..Default::default()
        };
        let out = style_name("file.rs", &style);
        assert_eq!(out, "file.rs", "plain when color off");
        colored::control::unset_override();
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib utils::tests::style_name`
Expected: FAIL — `cannot find function style_name in this scope`.

- [ ] **Step 4: Implement `style_name` in `src/utils.rs`**

Add this function to `src/utils.rs` (e.g. directly above the `#[cfg(test)] mod tests` block):

```rust
/// Renders a filename with an LsColors style applied (foreground color +
/// bold/italic/underline). Goes through the `colored` crate, so it honors the
/// global color override / TTY detection: when color is disabled the result is
/// the plain name.
pub fn style_name(name: &str, style: &lscolors::Style) -> String {
    use colored::Colorize;
    let mut styled = name.normal();
    if let Some(fg) = style.foreground {
        use lscolors::Color as LsColor;
        let color = match fg {
            LsColor::Black => colored::Color::Black,
            LsColor::Red => colored::Color::Red,
            LsColor::Green => colored::Color::Green,
            LsColor::Yellow => colored::Color::Yellow,
            LsColor::Blue => colored::Color::Blue,
            LsColor::Magenta => colored::Color::Magenta,
            LsColor::Cyan => colored::Color::Cyan,
            LsColor::White => colored::Color::White,
            LsColor::BrightBlack => colored::Color::BrightBlack,
            LsColor::BrightRed => colored::Color::BrightRed,
            LsColor::BrightGreen => colored::Color::BrightGreen,
            LsColor::BrightYellow => colored::Color::BrightYellow,
            LsColor::BrightBlue => colored::Color::BrightBlue,
            LsColor::BrightMagenta => colored::Color::BrightMagenta,
            LsColor::BrightCyan => colored::Color::BrightCyan,
            LsColor::BrightWhite => colored::Color::BrightWhite,
            LsColor::Fixed(_) => colored::Color::White,
            LsColor::RGB(r, g, b) => colored::Color::TrueColor { r, g, b },
        };
        styled = styled.color(color);
    }
    if style.font_style.bold {
        styled = styled.bold();
    }
    if style.font_style.italic {
        styled = styled.italic();
    }
    if style.font_style.underline {
        styled = styled.underline();
    }
    styled.to_string()
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib utils::tests::style_name`
Expected: PASS (2 tests).

- [ ] **Step 6: Refactor `view.rs` to use the helper**

In `src/view.rs`, replace lines 195–231 (from `let ls_style = ...` through the `if ls_style.font_style.underline { ... }` block) with:

```rust
        let ls_style = ls_colors.style_for_path(entry.path()).cloned().unwrap_or_default();
        let styled_name = utils::style_name(name.as_ref(), &ls_style);
```

(`name` is a `Cow<str>`, so `name.as_ref()` yields `&str`. `styled_name` is now a `String`; the existing `final_name` block below it — `format!("...{styled_name}...")` and `styled_name.to_string()` — continues to compile unchanged.)

- [ ] **Step 7: Verify the full suite + lint**

Run: `cargo test` → all pass (existing view behavior preserved + 2 new).
Run: `just lint` → no warnings (confirm no unused `use lscolors::Color`/`colored` imports remain in `view.rs`; the file still uses `colored` for the status-char block and `lscolors::LsColors` for the param type).

- [ ] **Step 8: Commit**

```bash
git add src/lib.rs src/utils.rs src/view.rs
git commit -m "refactor: extract utils::style_name shared by both views"
```

---

## Task 2: Color marks, churn, and filenames in `TerminalRenderer`

Add color to the comparison-mode renderer and thread `LsColors` + scope root through from `main.rs`.

**Files:**
- Modify: `src/lib.rs` (`TerminalRenderer` struct/impl, `render`, `node`, new `mark_color`/`add_str`/`del_str`, color tests)
- Modify: `src/main.rs` (construction site ~line 155)

**Interfaces:**
- Consumes: `crate::style_name` and `crate::test_color::guard` (Task 1, same library crate).
- Produces: `pub struct TerminalRenderer<'a> { pub marks: MarkScheme, pub format: OutputFormat, pub ls_colors: &'a lscolors::LsColors, pub root: std::path::PathBuf }`.

- [ ] **Step 1: Write the failing color tests**

Append to `src/lib.rs`:

```rust
#[cfg(test)]
mod color_tests {
    use super::*;

    fn sample_tree() -> ChangeTree {
        let staged = TreeNode {
            name: "a.rs".into(),
            path: "a.rs".into(),
            kind: NodeKind::File,
            status: ChangeStatus::Staged,
            churn: Churn { added: 3, deleted: 1 },
            rollup: Rollup::default(),
            children: vec![],
        };
        let deleted = TreeNode {
            name: "gone.rs".into(),
            path: "gone.rs".into(),
            kind: NodeKind::File,
            status: ChangeStatus::Deleted,
            churn: Churn { added: 0, deleted: 5 },
            rollup: Rollup::default(),
            children: vec![],
        };
        let summary = Rollup { dirs_touched: 0, files_changed: 2, churn: Churn { added: 3, deleted: 6 } };
        let root = TreeNode {
            name: "repo".into(),
            path: "".into(),
            kind: NodeKind::Directory,
            status: ChangeStatus::Clean,
            churn: Churn::default(),
            rollup: summary.clone(),
            children: vec![staged, deleted],
        };
        ChangeTree {
            schema_version: SCHEMA_VERSION.into(),
            comparison: ComparisonMode::Staged,
            view: View::BlastRadius,
            root,
            summary,
            fallback: None,
        }
    }

    #[test]
    fn color_on_emits_ansi_for_marks_and_churn() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let lsc = lscolors::LsColors::empty();
        let r = TerminalRenderer {
            marks: MarkScheme::Symbol,
            format: OutputFormat::Pretty,
            ls_colors: &lsc,
            root: std::path::PathBuf::from("/no/such/root"),
        };
        let out = r.render(&sample_tree()).unwrap();
        assert!(out.contains("\x1b[32m"), "green present (staged mark / +N): {out:?}");
        assert!(out.contains("\x1b[31m"), "red present (deleted mark / −M): {out:?}");
        colored::control::unset_override();
    }

    #[test]
    fn color_off_is_plain() {
        let _c = crate::test_color::guard();
        colored::control::set_override(false);
        let lsc = lscolors::LsColors::empty();
        let r = TerminalRenderer {
            marks: MarkScheme::Symbol,
            format: OutputFormat::Pretty,
            ls_colors: &lsc,
            root: std::path::PathBuf::from("/no/such/root"),
        };
        let out = r.render(&sample_tree()).unwrap();
        assert!(!out.contains("\x1b["), "no ANSI when color off: {out:?}");
        colored::control::unset_override();
    }

    #[test]
    fn filenames_use_lscolors() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.rs"), "x").unwrap();
        let lsc = lscolors::LsColors::from_string("*.rs=35"); // magenta foreground
        let r = TerminalRenderer {
            marks: MarkScheme::Symbol,
            format: OutputFormat::Pretty,
            ls_colors: &lsc,
            root: tmp.path().to_path_buf(),
        };
        let out = r.render(&sample_tree()).unwrap();
        assert!(out.contains("\x1b[35m"), "filename magenta from LS_COLORS: {out:?}");
        colored::control::unset_override();
    }

    #[test]
    fn deleted_missing_path_does_not_panic() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let lsc = lscolors::LsColors::empty();
        let r = TerminalRenderer {
            marks: MarkScheme::Symbol,
            format: OutputFormat::Pretty,
            ls_colors: &lsc,
            root: std::path::PathBuf::from("/no/such/root"),
        };
        // gone.rs does not exist under root → style lookup must fall back, not panic.
        let _ = r.render(&sample_tree()).unwrap();
        colored::control::unset_override();
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib color_tests`
Expected: FAIL — `TerminalRenderer` has no fields `ls_colors`/`root` (struct mismatch / missing lifetime).

- [ ] **Step 3: Add the `colored` import to `src/lib.rs`**

Add near the top of `src/lib.rs` (with the other `use` lines):

```rust
use colored::Colorize;
```

- [ ] **Step 4: Change the `TerminalRenderer` struct + impl headers**

In `src/lib.rs`, replace:

```rust
#[derive(Debug, Clone)]
pub struct TerminalRenderer {
    pub marks: MarkScheme,
    pub format: OutputFormat,
}
```

with:

```rust
#[derive(Debug, Clone)]
pub struct TerminalRenderer<'a> {
    pub marks: MarkScheme,
    pub format: OutputFormat,
    pub ls_colors: &'a lscolors::LsColors,
    pub root: PathBuf,
}
```

Then change the two impl headers:
- `impl Renderer for TerminalRenderer {` → `impl Renderer for TerminalRenderer<'_> {`
- `impl TerminalRenderer {` → `impl TerminalRenderer<'_> {`

- [ ] **Step 5: Add the color helpers**

In `src/lib.rs`, add these free functions next to `fn mark(...)`:

```rust
fn mark_color(s: &ChangeStatus) -> Option<colored::Color> {
    use colored::Color;
    match s {
        ChangeStatus::Staged => Some(Color::Green),
        ChangeStatus::Unstaged => Some(Color::Yellow),
        ChangeStatus::Both => Some(Color::Cyan),
        ChangeStatus::Untracked => Some(Color::Magenta),
        ChangeStatus::Renamed => Some(Color::Blue),
        ChangeStatus::Deleted => Some(Color::Red),
        ChangeStatus::Ignored => Some(Color::BrightBlack),
        ChangeStatus::Clean => None,
    }
}
fn add_str(n: usize) -> String {
    format!("+{n}").green().to_string()
}
fn del_str(n: usize) -> String {
    format!("−{n}").red().to_string()
}
```

- [ ] **Step 6: Color the summary footer in `render`**

In `TerminalRenderer::render`, replace:

```rust
        out.push_str(&format!(
            "\n{} dirs touched · {} files changed · +{} −{}\n",
            tree.summary.dirs_touched,
            tree.summary.files_changed,
            tree.summary.churn.added,
            tree.summary.churn.deleted
        ));
```

with:

```rust
        out.push_str(&format!(
            "\n{} dirs touched · {} files changed · {} {}\n",
            tree.summary.dirs_touched,
            tree.summary.files_changed,
            add_str(tree.summary.churn.added),
            del_str(tree.summary.churn.deleted)
        ));
```

- [ ] **Step 7: Color marks, churn, and filenames in `node`**

In `src/lib.rs`, replace the body of `fn node` (the whole method) with:

```rust
    fn node(&self, out: &mut String, n: &TreeNode, prefix: &str, last: bool) {
        let conn = if last { "└──" } else { "├──" };
        let mark_str = mark(n, self.marks);
        let mark_render = match mark_color(&n.status) {
            Some(c) => mark_str.color(c).to_string(),
            None => mark_str.to_string(),
        };
        let metric = if n.kind == NodeKind::Directory {
            format!(
                " ({} files, {} {})",
                n.rollup.files_changed,
                add_str(n.rollup.churn.added),
                del_str(n.rollup.churn.deleted)
            )
        } else {
            format!(" {} {}", add_str(n.churn.added), del_str(n.churn.deleted))
        };
        let abs = self.root.join(&n.path);
        let style = self.ls_colors.style_for_path(&abs).cloned().unwrap_or_default();
        let name_render = crate::style_name(&n.name, &style);
        out.push_str(&format!("{prefix}{conn} {mark_render} {name_render}{metric}\n"));
        let next = format!("{}{}", prefix, if last { "    " } else { "│   " });
        for (idx, c) in n.children.iter().enumerate() {
            self.node(out, c, &next, idx + 1 == n.children.len());
        }
    }
```

- [ ] **Step 8: Run the color tests to verify they pass**

Run: `cargo test --lib color_tests`
Expected: PASS (4 tests). If they fail to compile because `main.rs` still constructs the old struct shape, that is expected until Step 9.

- [ ] **Step 9: Wire `main.rs` to pass `ls_colors` + root**

In `src/main.rs`, replace the renderer construction line (currently `print!("{}", TerminalRenderer { marks, format }.render(&tree)?);`) with:

```rust
        let render_root =
            std::fs::canonicalize(&view_args.path).unwrap_or_else(|_| view_args.path.clone());
        print!(
            "{}",
            TerminalRenderer { marks, format, ls_colors, root: render_root }.render(&tree)?
        );
```

(`ls_colors` is the `&LsColors` parameter already in scope in `run_cli`.)

- [ ] **Step 10: Run the full suite + lint + visual check**

Run: `cargo test` → all pass.
Run: `just lint` → no warnings.
Run: `cargo run -- --pr --committed` inside this repo → confirm marks, `+N`/`−M`, and filenames are colored in the terminal; `cargo run -- --pr --committed | cat` → confirm plain (no ANSI) when piped.

- [ ] **Step 11: Commit**

```bash
git add src/lib.rs src/main.rs
git commit -m "feat: color marks, churn, and filenames in comparison views"
```

---

## Task 3: Document color in the README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a color note**

In `README.md`, in the comparison-modes area (near the `--pr` bullet added earlier), add:

```markdown
- Comparison views (`--pr`, `--staged`, `--all`, …) are colorized when color is enabled
  (status marks by git state, `+N` green / `−M` red churn, and filenames via `LS_COLORS`).
  Honors `--color=<when>`, `--force-color`/`-C`, `NO_COLOR`, and auto-disables when piped.
```

- [ ] **Step 2: Sanity build & commit**

Run: `cargo build` (markdown-only change; confirm nothing else broke).

```bash
git add README.md
git commit -m "docs: note comparison-view colorization"
```

---

## Self-Review

**Spec coverage:**
- Marks colored by `ChangeStatus` (palette) → Task 2 (`mark_color`, Step 5/7). ✓
- Churn `+`green/`−`red at all three sites → Task 2 (`add_str`/`del_str`, Steps 5–7). ✓
- Filenames via LsColors → Task 2 (Step 7, `utils::style_name`) on the shared helper from Task 1. ✓
- Gating via existing `colored` override (no `--heat`) → coloring routed through `colored`; tests prove on/off (Task 2 Steps 1/8; Task 1 Steps 2/5). ✓
- Renamed/deleted edge cases → palette covers them; `deleted_missing_path_does_not_panic` (Task 2) proves graceful fallback. ✓
- DRY shared helper + `view.rs` refactor → Task 1. ✓
- Renderer interface change (`ls_colors`/`root`) + `main.rs` wiring → Task 2 Steps 4/9. ✓
- README → Task 3. ✓
- JSON untouched → `JsonRenderer` not modified by any task. ✓
- Out of scope (bar/badge, `--heat` parsing) → not implemented. ✓

**Placeholder scan:** No TBD/TODO/vague steps; every code step shows complete code. ✓

**Type consistency:** `style_name(&str, &lscolors::Style) -> String` is identical across Task 1 (def) and Task 2 (call with `&n.name`). `TerminalRenderer<'a> { marks, format, ls_colors: &'a LsColors, root: PathBuf }` is consistent across Task 2's struct def, the four color tests, and the `main.rs` construction. `mark_color(&ChangeStatus) -> Option<colored::Color>`, `add_str(usize)->String`, `del_str(usize)->String` are defined once (Task 2 Step 5) and used in Steps 6–7. `crate::test_color::guard()` defined in Task 1 Step 1, used in all color tests. ✓
