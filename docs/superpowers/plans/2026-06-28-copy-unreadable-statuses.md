# Copy And Unreadable Statuses Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-class copied and unreadable statuses to difftree output, JSON, summaries, and help text.

**Architecture:** Reuse the existing `ChangeStatus` pipeline in `src/lib.rs`: git deltas become `FileChange`, `FileChange` becomes `TreeNode`, and renderers derive marks/colors/display text from `TreeNode`. Copy reuses the existing `old_path` field as the source path and displays as `source => copy`; unreadable paths carry zero textual churn and render as a visible read-error state instead of disappearing or looking like ordinary untracked files.

**Tech Stack:** Rust, `git2` similarity detection, `ignore` filesystem walking, `colored`, `clap`, `assert_cmd`, `tempfile`.

---

## Decisions

- `ChangeStatus::Copied` is a first-class status, not a variant of renamed. It uses `old_path` for the copy source.
- Copy display is `source => copy` so it is visually distinct from rename display, which remains `old -> new`.
- Single-letter marks use `C` for copied because that matches Git's copy status. Conflicted changes move from `C` to `U` in the letter scheme because `U` is the conventional "unmerged" marker and `--marks=xy` already uses `UU`.
- Unreadable uses `E` in single-letter and XY modes because the important user-facing meaning is "read error." This avoids overloading `U`, which already means unmerged/conflicted.
- Unreadable files have zero churn. If difftree cannot read the file, it cannot count text lines accurately.
- Binary untracked files are still `Untracked` with zero churn. Only a failed filesystem read becomes `Unreadable`.
- First pass scope covers libgit2 `Delta::Unreadable` and unreadable untracked files. Unreadable directories from walker errors are not modeled until the tree model has a directory-level changed status or an unknown-node kind.

## Status Key After This Plan

| Status | Symbol | Letter | XY | Color | Meaning |
| --- | --- | --- | --- | --- | --- |
| Staged | `●` | `S` | `M ` | green | Change is in the git index. |
| Unstaged | `○` | `M` | ` M` | yellow | Tracked worktree change is not staged. |
| Both | `◐` | `B` | `MM` | cyan | Same path has staged and unstaged changes. |
| Untracked | `?` | `?` | `??` | magenta | Path is not tracked by git. |
| Renamed | `↻` | `R` | `R ` | blue | Tracked path moved or renamed; shown as `old -> new`. |
| Copied | `⧉` | `C` | `C ` | bright blue | New path was detected as a copy of another path; shown as `source => copy`. |
| Deleted | `×` | `D` | `D ` | red | Tracked path was removed. |
| Typechanged | `◆` | `T` | `T ` | cyan | Tracked object kind changed, such as file to symlink or submodule. |
| Conflicted | `‼` | `U` | `UU` | bright red | Unmerged path during merge, rebase, or cherry-pick. |
| Unreadable | `⚠` | `E` | `E?` | bright yellow | Path exists but difftree could not read it. |
| Ignored | `!` | `I` | `!!` | bright black | Path matches ignore rules when ignored entries are shown. |
| Clean | blank | blank | blank | default | Unchanged path shown in all-files views. |

## Files

- Modify `src/lib.rs`: add `Copied` and `Unreadable`, update mark/color/display helpers, enable copy detection, map `git2::Delta::Copied` and `git2::Delta::Unreadable`, preserve copy source path, classify unreadable untracked files, and add focused unit tests.
- Modify `src/app.rs`: extend `STATUS_KEY_HELP` with copied and unreadable definitions, marks, and colors.
- Modify `tests/cli.rs`: extend `help_includes_status_key_and_definitions` so help output is pinned for the new statuses.

## Task 1: Status Vocabulary And Renderer Contract

**Files:**
- Modify: `src/lib.rs`
- Test: `src/lib.rs`

- [ ] **Step 1: Add failing renderer tests**

Add these tests in `src/lib.rs` inside `mod pr_tests`, near the existing rename/typechange/conflict tests:

```rust
    #[test]
    fn copied_display_name_uses_source_arrow() {
        let node = TreeNode {
            name: "copy.txt".to_string(),
            path: "copy.txt".to_string(),
            old_path: Some("source.txt".to_string()),
            kind: NodeKind::File,
            status: ChangeStatus::Copied,
            churn: Churn::default(),
            rollup: Rollup::default(),
            children: vec![],
        };

        assert_eq!(display_name(&node), "source.txt => copy.txt");
    }

    #[test]
    fn copied_and_unreadable_marks_are_distinct() {
        let copied = TreeNode {
            name: "copy.txt".to_string(),
            path: "copy.txt".to_string(),
            old_path: Some("source.txt".to_string()),
            kind: NodeKind::File,
            status: ChangeStatus::Copied,
            churn: Churn::default(),
            rollup: Rollup::default(),
            children: vec![],
        };
        let unreadable = TreeNode {
            name: "locked.txt".to_string(),
            path: "locked.txt".to_string(),
            old_path: None,
            kind: NodeKind::File,
            status: ChangeStatus::Unreadable,
            churn: Churn::default(),
            rollup: Rollup::default(),
            children: vec![],
        };
        let conflict = TreeNode {
            name: "conflict.txt".to_string(),
            path: "conflict.txt".to_string(),
            old_path: None,
            kind: NodeKind::File,
            status: ChangeStatus::Conflicted,
            churn: Churn::default(),
            rollup: Rollup::default(),
            children: vec![],
        };

        assert_eq!(mark(&copied, MarkScheme::Letter), "C");
        assert_eq!(mark(&copied, MarkScheme::Xy), "C ");
        assert_eq!(mark(&unreadable, MarkScheme::Letter), "E");
        assert_eq!(mark(&unreadable, MarkScheme::Xy), "E?");
        assert_eq!(mark(&conflict, MarkScheme::Letter), "U");
        assert_eq!(mark(&conflict, MarkScheme::Xy), "UU");
    }
```

- [ ] **Step 2: Run the failing tests**

Run:

```bash
cargo test copied --lib
```

Expected result: compile fails because `ChangeStatus::Copied` and `ChangeStatus::Unreadable` do not exist.

- [ ] **Step 3: Add the status variants**

Update `ChangeStatus` in `src/lib.rs`:

```rust
pub enum ChangeStatus {
    Staged,
    Unstaged,
    Both,
    Untracked,
    Renamed,
    Copied,
    Deleted,
    Typechanged,
    Conflicted,
    Unreadable,
    Ignored,
    Clean,
}
```

- [ ] **Step 4: Update renderer colors**

Update `mark_color` in `src/lib.rs`:

```rust
fn mark_color(s: &ChangeStatus) -> Option<colored::Color> {
    use colored::Color;
    match s {
        ChangeStatus::Staged => Some(Color::Green),
        ChangeStatus::Unstaged => Some(Color::Yellow),
        ChangeStatus::Both => Some(Color::Cyan),
        ChangeStatus::Untracked => Some(Color::Magenta),
        ChangeStatus::Renamed => Some(Color::Blue),
        ChangeStatus::Copied => Some(Color::BrightBlue),
        ChangeStatus::Deleted => Some(Color::Red),
        ChangeStatus::Typechanged => Some(Color::Cyan),
        ChangeStatus::Conflicted => Some(Color::BrightRed),
        ChangeStatus::Unreadable => Some(Color::BrightYellow),
        ChangeStatus::Ignored => Some(Color::BrightBlack),
        ChangeStatus::Clean => None,
    }
}
```

- [ ] **Step 5: Update renderer marks**

Update `mark` in `src/lib.rs`:

```rust
fn mark(n: &TreeNode, s: MarkScheme) -> &'static str {
    match s {
        MarkScheme::Symbol => match n.status {
            ChangeStatus::Staged => "●",
            ChangeStatus::Unstaged => "○",
            ChangeStatus::Both => "◐",
            ChangeStatus::Untracked => "?",
            ChangeStatus::Renamed => "↻",
            ChangeStatus::Copied => "⧉",
            ChangeStatus::Deleted => "×",
            ChangeStatus::Typechanged => "◆",
            ChangeStatus::Conflicted => "‼",
            ChangeStatus::Unreadable => "⚠",
            ChangeStatus::Ignored => "!",
            ChangeStatus::Clean => " ",
        },
        MarkScheme::Letter => match n.status {
            ChangeStatus::Staged => "S",
            ChangeStatus::Unstaged => "M",
            ChangeStatus::Both => "B",
            ChangeStatus::Untracked => "?",
            ChangeStatus::Renamed => "R",
            ChangeStatus::Copied => "C",
            ChangeStatus::Deleted => "D",
            ChangeStatus::Typechanged => "T",
            ChangeStatus::Conflicted => "U",
            ChangeStatus::Unreadable => "E",
            ChangeStatus::Ignored => "I",
            ChangeStatus::Clean => " ",
        },
        MarkScheme::Xy => match n.status {
            ChangeStatus::Staged => "M ",
            ChangeStatus::Unstaged => " M",
            ChangeStatus::Both => "MM",
            ChangeStatus::Untracked => "??",
            ChangeStatus::Renamed => "R ",
            ChangeStatus::Copied => "C ",
            ChangeStatus::Deleted => "D ",
            ChangeStatus::Typechanged => "T ",
            ChangeStatus::Conflicted => "UU",
            ChangeStatus::Unreadable => "E?",
            ChangeStatus::Ignored => "!!",
            ChangeStatus::Clean => "  ",
        },
    }
}
```

- [ ] **Step 6: Update display names for copied paths**

Replace `display_name` in `src/lib.rs` with:

```rust
fn display_name(n: &TreeNode) -> String {
    if matches!(n.status, ChangeStatus::Renamed | ChangeStatus::Copied) {
        if let Some(old_path) = &n.old_path {
            let old = Path::new(old_path);
            let new = Path::new(&n.path);
            let same_parent =
                old.parent().unwrap_or(Path::new("")) == new.parent().unwrap_or(Path::new(""));
            let old_display = if same_parent {
                old.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| old_path.clone())
            } else {
                old_path.clone()
            };
            let new_display = if same_parent { n.name.clone() } else { n.path.clone() };
            let arrow = if n.status == ChangeStatus::Renamed { " -> " } else { " => " };
            return format!("{old_display}{arrow}{new_display}");
        }
    }
    n.name.clone()
}
```

- [ ] **Step 7: Run renderer tests**

Run:

```bash
cargo test copied --lib
```

Expected result: both tests pass.

## Task 2: Copy Detection In Git Diffs

**Files:**
- Modify: `src/lib.rs`
- Test: `src/lib.rs`

- [ ] **Step 1: Add a file-node lookup helper for tests**

Add this helper in `src/lib.rs` inside `mod pr_tests`, near `find_file_churn`:

```rust
    fn find_file_node<'a>(tree: &'a ChangeTree, name: &str) -> Option<&'a TreeNode> {
        fn walk<'a>(n: &'a TreeNode, name: &str) -> Option<&'a TreeNode> {
            if n.kind == NodeKind::File && n.name == name {
                return Some(n);
            }
            for c in &n.children {
                if let Some(found) = walk(c, name) {
                    return Some(found);
                }
            }
            None
        }
        walk(&tree.root, name)
    }
```

- [ ] **Step 2: Add a failing copy-detection test**

Add this test in `src/lib.rs` inside `mod pr_tests`, near `staged_rename_renders_as_single_rename`:

```rust
    #[test]
    fn staged_copy_renders_as_single_copy_with_source_path() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("source.txt"), "same\nsame\nsame\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::copy(p.join("source.txt"), p.join("copy.txt")).unwrap();
        git(p, &["add", "copy.txt"]);

        let tree = collect_changes(p, ComparisonMode::Staged, false).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        assert!(out.contains("C source.txt => copy.txt"), "{out}");
        assert!(!out.contains("S copy.txt"), "{out}");
        assert_eq!(tree.summary.files_changed, 1);
        let node = find_file_node(&tree, "copy.txt").expect("copy.txt present");
        assert_eq!(node.status, ChangeStatus::Copied);
        assert_eq!(node.old_path.as_deref(), Some("source.txt"));
    }
```

- [ ] **Step 3: Run the failing copy test**

Run:

```bash
cargo test staged_copy_renders_as_single_copy_with_source_path --lib
```

Expected result: the test fails because `copy.txt` renders as an ordinary staged addition.

- [ ] **Step 4: Enable libgit2 copy detection**

Update `build_diff` in `src/lib.rs`:

```rust
    let mut find = DiffFindOptions::new();
    find.renames(true);
    find.copies(true);
    find.copies_from_unmodified(true);
    diff.find_similar(Some(&mut find))?;
```

- [ ] **Step 5: Extract status mapping**

Add this helper above `diff_files` in `src/lib.rs`:

```rust
fn status_for_delta(delta: git2::Delta, mode: &ComparisonMode) -> ChangeStatus {
    match delta {
        git2::Delta::Deleted => ChangeStatus::Deleted,
        git2::Delta::Renamed => ChangeStatus::Renamed,
        git2::Delta::Copied => ChangeStatus::Copied,
        git2::Delta::Typechange => ChangeStatus::Typechanged,
        git2::Delta::Conflicted => ChangeStatus::Conflicted,
        _ => match mode {
            ComparisonMode::Unstaged => ChangeStatus::Unstaged,
            _ => ChangeStatus::Staged,
        },
    }
}
```

- [ ] **Step 6: Preserve old path for copies**

Update the relevant section in `diff_files` in `src/lib.rs`:

```rust
        let status = status_for_delta(delta.status(), mode);
        let old_path = if matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied) {
            delta.old_file().path().map(|p| p.to_path_buf())
        } else {
            None
        };
```

- [ ] **Step 7: Run copy and rename tests**

Run:

```bash
cargo test staged_copy_renders_as_single_copy_with_source_path --lib
cargo test staged_rename_renders_as_single_rename --lib
```

Expected result: both tests pass. Rename output remains `R old-name.txt -> new-name.txt`.

## Task 3: Unreadable Diff And Untracked File Handling

**Files:**
- Modify: `src/lib.rs`
- Test: `src/lib.rs`

- [ ] **Step 1: Add a failing delta-mapping test**

Add this test in `src/lib.rs` inside `mod pr_tests`:

```rust
    #[test]
    fn unreadable_delta_maps_to_unreadable_status() {
        assert_eq!(
            status_for_delta(git2::Delta::Unreadable, &ComparisonMode::Staged),
            ChangeStatus::Unreadable
        );
    }
```

- [ ] **Step 2: Add a failing unreadable-untracked test**

Add this Unix-only helper and test in `src/lib.rs` inside `mod pr_tests`:

```rust
    #[cfg(unix)]
    struct ModeGuard {
        path: PathBuf,
        mode: u32,
    }

    #[cfg(unix)]
    impl Drop for ModeGuard {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(self.mode));
        }
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_untracked_file_renders_read_error() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);

        let locked = p.join("locked.txt");
        std::fs::write(&locked, "secret\n").unwrap();
        let original_mode = std::fs::metadata(&locked).unwrap().permissions().mode();
        let _guard = ModeGuard { path: locked.clone(), mode: original_mode };
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        let tree = collect_changes(p, ComparisonMode::Staged, true).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        assert!(out.contains("E locked.txt"), "{out}");
        assert!(!out.contains("? locked.txt"), "{out}");
        let node = find_file_node(&tree, "locked.txt").expect("locked.txt present");
        assert_eq!(node.status, ChangeStatus::Unreadable);
        assert_eq!(node.churn, Churn::default());
    }
```

- [ ] **Step 3: Run the failing unreadable tests**

Run:

```bash
cargo test unreadable_delta_maps_to_unreadable_status --lib
cargo test unreadable_untracked_file_renders_read_error --lib
```

Expected result: the delta test fails until `Delta::Unreadable` is mapped; the Unix untracked test fails because unreadable untracked files currently render as `? locked.txt` with zero churn.

- [ ] **Step 4: Map unreadable deltas**

Update `status_for_delta` in `src/lib.rs`:

```rust
fn status_for_delta(delta: git2::Delta, mode: &ComparisonMode) -> ChangeStatus {
    match delta {
        git2::Delta::Deleted => ChangeStatus::Deleted,
        git2::Delta::Renamed => ChangeStatus::Renamed,
        git2::Delta::Copied => ChangeStatus::Copied,
        git2::Delta::Typechange => ChangeStatus::Typechanged,
        git2::Delta::Conflicted => ChangeStatus::Conflicted,
        git2::Delta::Unreadable => ChangeStatus::Unreadable,
        _ => match mode {
            ComparisonMode::Unstaged => ChangeStatus::Unstaged,
            _ => ChangeStatus::Staged,
        },
    }
}
```

- [ ] **Step 5: Avoid patch extraction for unreadable deltas**

Update churn extraction in `diff_files`:

```rust
        let churn = if status == ChangeStatus::Unreadable {
            Churn::default()
        } else {
            match git2::Patch::from_diff(&diff, idx)? {
                Some(patch) => {
                    let (_context, added, deleted) = patch.line_stats()?;
                    Churn { added, deleted }
                }
                None => Churn::default(),
            }
        };
```

- [ ] **Step 6: Add a helper for untracked content reads**

Add this helper above `add_untracked` in `src/lib.rs`:

```rust
fn status_and_churn_for_untracked(abs: &Path) -> (ChangeStatus, Churn) {
    match std::fs::read(abs) {
        Ok(bytes) => {
            let added = String::from_utf8(bytes).map(|s| s.lines().count()).unwrap_or(0);
            (ChangeStatus::Untracked, Churn { added, deleted: 0 })
        }
        Err(_) => (ChangeStatus::Unreadable, Churn::default()),
    }
}
```

- [ ] **Step 7: Use the helper in `add_untracked`**

Update the body of the `if let Some(p) = e.path()` branch in `add_untracked`:

```rust
                let (status, churn) = workdir
                    .map(|w| status_and_churn_for_untracked(&w.join(p)))
                    .unwrap_or((ChangeStatus::Unreadable, Churn::default()));
                files.push(FileChange {
                    path: PathBuf::from(p),
                    old_path: None,
                    status,
                    churn,
                });
```

- [ ] **Step 8: Run unreadable and untracked churn tests**

Run:

```bash
cargo test unreadable_delta_maps_to_unreadable_status --lib
cargo test unreadable_untracked_file_renders_read_error --lib
cargo test churn_counts_lines_for_untracked_files --lib
```

Expected result: all listed tests pass. The existing untracked churn test proves readable untracked files still render as untracked and keep line counts.

## Task 4: Help Text And CLI Contract

**Files:**
- Modify: `src/app.rs`
- Modify: `tests/cli.rs`

- [ ] **Step 1: Add failing help assertions**

Extend `help_includes_status_key_and_definitions` in `tests/cli.rs`:

```rust
        .stdout(predicate::str::contains("copied"))
        .stdout(predicate::str::contains("source => copy"))
        .stdout(predicate::str::contains("unreadable"))
        .stdout(predicate::str::contains("could not read"))
        .stdout(predicate::str::contains("C copied"))
        .stdout(predicate::str::contains("U conflicted"))
        .stdout(predicate::str::contains("E unreadable"))
```

- [ ] **Step 2: Run the failing help test**

Run:

```bash
cargo test --test cli help_includes_status_key_and_definitions
```

Expected result: test fails because help does not mention copied or unreadable.

- [ ] **Step 3: Update `STATUS_KEY_HELP`**

Replace the mark and definition sections in `src/app.rs` with:

```rust
const STATUS_KEY_HELP: &str = r#"Status key:
  Row format: <tree connector> <status mark> <path> +added -deleted
  Directories show a rollup: (<changed files> files, +added -deleted)

  --marks=symbol: ● staged, ○ unstaged, ◐ staged+unstaged, ? untracked,
                  ↻ renamed, ⧉ copied, × deleted, ◆ typechanged,
                  ‼ conflicted, ⚠ unreadable, ! ignored
  --marks=letter: S staged, M unstaged, B staged+unstaged, ? untracked,
                  R renamed, C copied, D deleted, T typechanged,
                  U conflicted, E unreadable, I ignored
  --marks=xy:     M_ staged, _M unstaged, MM staged+unstaged, ?? untracked,
                  R_ renamed, C_ copied, D_ deleted, T_ typechanged,
                  UU conflicted, E? unreadable, !! ignored
                  (_ means a space; clean files have a blank mark)

Definitions:
  staged: change is in the git index.
  unstaged: tracked worktree change is not staged.
  staged+unstaged: the same path has both staged and unstaged changes.
  untracked: path is not tracked by git.
  renamed: tracked path moved or renamed; shown as old -> new.
  copied: new path was detected as a copy; shown as source => copy.
  deleted: tracked path was removed.
  typechanged: tracked object kind changed, such as file <-> symlink or submodule.
  conflicted: unmerged path during merge/rebase/cherry-pick.
  unreadable: path exists but difftree could not read it.
  ignored: path matches git ignore rules when ignored entries are shown.
  clean: unchanged path shown in all-files views.

Colors when enabled: staged green, unstaged yellow, staged+unstaged cyan,
  untracked magenta, renamed blue, copied bright blue, deleted red,
  typechanged cyan, conflicted bright red, unreadable bright yellow,
  ignored gray. Churn is +added green and -deleted red; filenames follow
  LS_COLORS."#;
```

- [ ] **Step 4: Run the help test**

Run:

```bash
cargo test --test cli help_includes_status_key_and_definitions
```

Expected result: test passes.

## Task 5: Full Verification

**Files:**
- Verify: `src/lib.rs`
- Verify: `src/app.rs`
- Verify: `tests/cli.rs`

- [ ] **Step 1: Format the Rust code**

Run:

```bash
cargo fmt
```

Expected result: command exits successfully and formats modified Rust files.

- [ ] **Step 2: Run focused library tests**

Run:

```bash
cargo test staged_copy_renders_as_single_copy_with_source_path --lib
cargo test unreadable_untracked_file_renders_read_error --lib
```

Expected result: both tests pass on Unix. On non-Unix, the unreadable integration test is not compiled, and the copy test passes.

- [ ] **Step 3: Run status-regression library tests**

Run:

```bash
cargo test staged_rename_renders_as_single_rename --lib
cargo test staged_typechange_renders_with_typechange_marker --lib
cargo test merge_conflict_renders_with_conflict_marker --lib
cargo test churn_counts_lines_for_untracked_files --lib
```

Expected result: all tests pass. The conflict test should now expect `U conflict.txt` because the letter mark moves from `C` to `U`.

- [ ] **Step 4: Run the CLI help regression**

Run:

```bash
cargo test --test cli help_includes_status_key_and_definitions
```

Expected result: test passes and help includes copied, unreadable, and the full status key.

- [ ] **Step 5: Run the full suite**

Run:

```bash
cargo test
```

Expected result: all tests pass.

- [ ] **Step 6: Inspect the final diff**

Run:

```bash
git diff -- src/lib.rs src/app.rs tests/cli.rs
```

Expected result: diff is limited to copied/unreadable status support, the conflict letter-mark adjustment, and help/test updates. It does not alter PR base resolution, PR argument parsing, deleted-file handling, or committed PR tree filtering.
