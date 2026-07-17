# setup-difftree

A **pointer** skill: difftree's canonical install/setup skill lives in the
companion [difftree-action](https://github.com/smorinlabs/difftree-action) repo.
This skill installs and configures nothing itself — when, working in the difftree
repo, you ask to "install difftree", "set up difftree", or "add PR diff-tree
comments", it routes you to the canonical `setup-difftree` skill (which installs
the CLI and scaffolds the PR workflow) and to this repo's README for a plain
local `cargo install difftree`.

**Triggers on:** "install difftree", "set up difftree", "add difftree to my CI",
"add PR diff-tree comments", "set up difftree-action".
**Arguments:** none (informational — routes onward).

## Install

**In this repo — nothing to install.** Claude Code auto-discovers
`.claude/skills/setup-difftree/`; Codex discovers it through the committed
symlink `.agents/skills/setup-difftree`.

**Copy into your own setup** (no dependencies):

    git clone https://github.com/smorinlabs/difftree
    cp -R difftree/.claude/skills/setup-difftree ~/.claude/skills/setup-difftree   # Claude Code
    cp -R difftree/.claude/skills/setup-difftree ~/.agents/skills/setup-difftree   # Codex

**Dev mode** (edits in the clone are live next session):

    ln -s "$(pwd)/difftree/.claude/skills/setup-difftree" ~/.claude/skills/setup-difftree   # Claude Code
    ln -s "$(pwd)/difftree/.claude/skills/setup-difftree" ~/.agents/skills/setup-difftree   # Codex

## The real setup skill

For the actual CLI install and CI wiring, use the canonical skill in the
difftree-action repo:
<https://github.com/smorinlabs/difftree-action/blob/main/docs/skills/setup-difftree.md>.

## Example session

> Add difftree PR comments to this repo.
> → Points you to the difftree-action `setup-difftree` skill and where to get
> that repo; for a plain local install, points at this repo's Installation
> section (`cargo install difftree`).
