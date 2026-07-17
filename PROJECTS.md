# Projects

## [~] Project P01: setup-difftree pointer skill (v0.3.1 — docs/tooling, no crate change)
**Goal**: Ship a lightweight in-repo `setup-difftree` pointer skill that routes
users to the canonical setup skill in the companion difftree-action repo, plus a
README note and a docs page. Documentation/tooling only — the crate is untouched,
so this rides the current version and triggers no release.

**Out of Scope**
- The canonical install/CI-wiring logic (lives in `smorinlabs/difftree-action`).
- Any change to the difftree CLI itself.

### Tests & Tasks
- [x] [P01-T01] Author `.claude/skills/setup-difftree/SKILL.md` (pure pointer) +
      committed `.agents/skills/setup-difftree` symlink (Codex discovery)
- [x] [P01-T02] Add `docs/skills/setup-difftree.md` and a README pointer note
- [x] [P01-TS01] `skill-quality` gate on the pointer skill passes
      (frontmatter ✓, docs ✓, `skillsmith verify` pass on claude-code + codex)
- [ ] [P01-T03] Open PR to `smorinlabs/difftree`
