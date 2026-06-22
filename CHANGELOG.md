# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-06-21

Initial release. difftree begins as a clean, credited fork of
[lstr](https://github.com/bgreenwell/lstr) v0.2.1 (commit `7e52218`) by Brandon
Greenwell, used under the MIT License. See `NOTICE` for provenance.

### Added

- Seeded the project from lstr v0.2.1 with fresh git history and full
  attribution (`LICENSE`, `NOTICE`, README credits).

### Changed

- Renamed the crate and binary from `lstr` to `difftree`. Behavior is otherwise
  identical to the upstream seed; git-aware tree features are the planned
  direction.

## Unreleased

### Added

- All-files view (`--all`, alias `--tree`): renders the complete directory tree
  with git change marks overlaid; unchanged files are shown as `Clean`.
- Explicit `--staged` (alias `--cached`) comparison flag.
- `--uncommitted` comparison (staged + unstaged + untracked vs HEAD), renamed
  from the previous `--all` comparison meaning.
- `view` field (`blast-radius` | `all-files`) in the `--json` model.

### Changed

- `--all` no longer means the combined comparison; use `--uncommitted` for that.
- `--tree` now shows every file (previously it rendered only changed files).
