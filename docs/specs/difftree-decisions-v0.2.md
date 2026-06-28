# difftree v0.2 implementation decisions

This document locks the PRD v0.2 build-phase details.

## Auto-fallback wording

When the default staged blast-radius view has no staged changes, difftree renders the unstaged comparison with this heading:

`No staged changes — showing unstaged blast radius`

## Heat flag grammar

`--heat=<components>` accepts a comma-separated list of `color`, `bar`, and `badge`. The default is `color,bar,badge`.

## Status marks

`--marks=symbol|letter|xy` selects the visible status scheme. The default is `symbol`.

| Status | Symbol | Letter | XY |
| --- | --- | --- | --- |
| staged | `●` | `S` | `M ` |
| unstaged | `○` | `M` | ` M` |
| both staged and further edited | `◐` | `B` | `MM` |
| untracked | `?` | `?` | `??` |
| renamed | `↻` | `R` | `R ` |
| deleted | `×` | `D` | `D ` |
| ignored | `!` | `I` | `!!` |

## Comparison precedence

When comparison flags are combined, difftree uses the first applicable mode in this order:

1. `--range <A..B>`
2. `--against <ref>`
3. `--uncommitted`
4. `--unstaged`
5. `--staged` (explicit; no auto-fallback)
6. default staged, with the documented unstaged fallback for the hero view

`tree` compatibility keeps `-a` as "show hidden files". The combined (HEAD vs working tree plus index) comparison is `--uncommitted`; `--all` (alias `--tree`) selects the all-files **view**, not a comparison.

## Flag table

| Flag | Meaning |
| --- | --- |
| `<path>` | Scope the selected view to a path. |
| `--all`, `--tree` | All-files view: every file, with change marks overlaid (Clean when unchanged). |
| `--plain`, `--no-git` | Classic tree mode with no git overlay. |
| `--staged`, `--cached` | Compare index to HEAD (explicit; no auto-fallback). |
| `--unstaged` | Compare working tree to index. |
| `--uncommitted` | Compare HEAD to working tree plus index (staged + unstaged + untracked). |
| `--range <A..B>` | Compare two revisions. |
| `--against <ref>` | Compare a ref to the working tree plus index. |
| `--json` | Emit JSON. Git-aware modes and views emit the shared `ChangeTree` model; plain/classic tree modes emit the plain-tree model. |
| `--format <pretty\|plain>` | Force terminal format. |
| `--color <auto\|always\|never>`, `--no-color`, `-n`, `-C` | Color control. |
| `--marks <symbol\|letter\|xy>` | Status mark scheme. |
| `--heat <components>` | Heat components. |
| `--show-ignored` | Show ignored entries inline. |
| `--ignored` | Dedicated ignored-file visualizer. |
| `-a` | Tree-compatible hidden files. |
| `-d` | Tree-compatible directories only. |
| `-f` | Reserved tree-compatible full-path output. |
| `-L <n>`, `--depth <n>` | Maximum traversal depth. |
| `-P <pattern>` / `-I <pattern>` | Reserved tree-compatible include/exclude patterns. |
| `--prune`, `--dirsfirst`, `--noreport`, `--filelimit` | Tree-compatible controls. |

## Summary frame

Every comparison terminal view renders one context header above the tree, after any fallback line:

| Mode | Header |
| --- | --- |
| `--pr` | `PR: {base_ref}...{head_label} · working tree` |
| `--pr --committed` | `PR: {base_ref}...{head_label} · committed` |
| `--against <ref>` | `Against: {ref}...working tree` |
| `--range <A..B>` | `Range: {A}..{B}` |
| `--staged` | `Staged changes` |
| `--unstaged` | `Unstaged changes` |
| `--uncommitted` | `Uncommitted changes (staged + unstaged)` |

`head_label` is the current branch name. Detached HEAD uses the shortest available
seven-character commit SHA. When `--pr` is already on the base branch, the header is
`PR: {base_ref}...{head_label} · on base branch (uncommitted only)`.

The footer keeps the existing `dirs touched` and churn segments. Its file-count
segment is kind-aware:

- Single kind: `N files modified`, `N files added`, etc.
- Mixed kinds: `N files changed (a added · b modified · c deleted · ...)`.
- Empty changes: `0 files changed`.

Display order is `added`, `modified`, `deleted`, `renamed`, `copied`,
`typechanged`, `conflicted`, then `unreadable`; zero counts are omitted.

## JSON schema

The v2 schema is versioned by `schema_version: "difftree.v2"`.

Schema decision: bump from v1 rather than add a silent default. v1 already used
`kind` for structural node type, while the comparison model now needs `kind` for
the git delta kind. In v2, structural node type is renamed to `node_kind`, and
changed file nodes expose the delta kind as `kind`.

The git-aware `ChangeTree` JSON model contains:

- `comparison`: the active comparison mode and parameters.
- `view`: the active view, `"blast-radius"` or `"all-files"`.
- `fallback`: null or the exact fallback heading.
- `root`: recursive tree nodes with `name`, `path`, `node_kind`, optional `old_path`,
  optional `kind`, `status`, `churn`, `rollup`, and `children`.
- `kind`: present on changed file nodes only; values are `added`, `modified`,
  `deleted`, `renamed`, `copied`, `typechanged`, `conflicted`, and `unreadable`.
- `summary`: repository/view-level `dirs_touched`, `files_changed`, and `churn` totals.

Plain-tree JSON is used when no git comparison is selected (`--plain`, non-git fallback,
`-G/--git-status`, and `interactive --json`). It uses the same
`schema_version: "difftree.v2"` and contains:

- `view: "plain-tree"`.
- `root`: recursive tree nodes with `name`, `path`, `node_kind`, optional `git_status`,
  optional `size_bytes`, optional `permissions`, and `children`.
- `summary`: `directories` and `files` counts for entries included after filters.

Explicit git comparison modes (`--staged`, `--unstaged`, `--uncommitted`, `--range`,
`--against`, and `--pr`) still require a git repository when combined with `--json`.
