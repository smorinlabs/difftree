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
| `--json` | Emit the JSON model. |
| `--format <pretty|plain>` | Force terminal format. |
| `--color <auto|always|never>`, `--no-color`, `-n`, `-C` | Color control. |
| `--marks <symbol|letter|xy>` | Status mark scheme. |
| `--heat <components>` | Heat components. |
| `--show-ignored` | Show ignored entries inline. |
| `--ignored` | Dedicated ignored-file visualizer. |
| `-a` | Tree-compatible hidden files. |
| `-d` | Tree-compatible directories only. |
| `-f` | Reserved tree-compatible full-path output. |
| `-L <n>`, `--depth <n>` | Maximum traversal depth. |
| `-P <pattern>` / `-I <pattern>` | Reserved tree-compatible include/exclude patterns. |
| `--prune`, `--dirsfirst`, `--noreport`, `--filelimit` | Tree-compatible controls. |

## JSON schema

The v1 schema is versioned by `schema_version: "difftree.v1"` and contains:

- `comparison`: the active comparison mode and parameters.
- `view`: the active view, `"blast-radius"` or `"all-files"`.
- `fallback`: null or the exact fallback heading.
- `root`: recursive tree nodes with `name`, `path`, `kind`, `status`, `churn`, `rollup`, and `children`.
- `summary`: repository/view-level `dirs_touched`, `files_changed`, and `churn` totals.

