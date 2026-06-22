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

- Added the v0.2 difftree PRD implementation foundation: serializable core model, renderer seam, JSON output, git comparison modes, status-marked terminal rendering, and a documented decisions/spec contract.
