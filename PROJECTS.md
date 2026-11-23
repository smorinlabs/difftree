# Projects

## [x] Project P01: setup-difftree pointer skill (v0.3.1 — docs/tooling, no crate change)
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
- [x] [P01-T03] Open PR to `smorinlabs/difftree` — merged as #15

## [~] Project P02: rename setup-difftree → difftree-setup (docs/tooling, no crate change)
**Goal**: Rename the pointer skill to `difftree-setup` so it no longer collides
with the difftree-action repo's canonical skill. **Skill names are a global
namespace** — two skills both named `setup-difftree` clobber at install
(`~/.claude/skills/setup-difftree`, `~/.agents/skills/setup-difftree`) and shadow
each other on any shared discovery path. The action repo's skill is renamed to
`difftree-action-setup` in parallel.

### Tests & Tasks
- [x] [P02-T01] `git mv` skill dir + docs page; retarget `.agents` symlink
- [x] [P02-T02] Update SKILL.md name/H1, cross-refs to `difftree-action-setup`,
      README link
- [x] [P02-TS01] `skillsmith verify` + skill-quality pass under the new name
      (pass on claude-code + codex; no stale refs; cross-refs resolve)
- [ ] [P02-T03] Open PR to `smorinlabs/difftree`

## [~] Project P03: CI speed — rust-cache, PR concurrency, Linux-only release build (ci, no crate change)
**Goal**: Cut "Rust CI" wall-clock (median 157 s, Windows on the critical path)
without dropping coverage or changing the three required check names.
- Replace `actions/cache` with `Swatinem/rust-cache@v2` (rustc-aware key,
  restore-keys fallback, `target/` pruning)
- Add a `concurrency` group that cancels superseded PR runs; main pushes are
  never cancelled
- Run the LTO `cargo build --release` step on the Linux leg only

**Out of Scope**
- cargo-nextest, path filters (all 3 legs are required checks), dropping Windows

### Tests & Tasks
- [x] [P03-T01] Edit `.github/workflows/ci.yml` (cache, concurrency, Build `if`)
- [x] [P03-TS01] `actionlint` passes (only the pre-existing Blacksmith
      runner-label warnings remain)
- [ ] [P03-TS02] CI green on all three legs; cold and warm timings recorded
- [ ] [P03-T02] Open PR to `smorinlabs/difftree`
