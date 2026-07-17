---
name: setup-difftree
description: >-
  Pointer to difftree's install/setup skill, which lives in the companion
  difftree-action repo. Fires when someone in the difftree repo says "install
  difftree", "set up difftree", "add difftree to my CI", "add PR diff-tree
  comments", or "set up difftree-action". It does not install or configure
  anything itself — it routes to the canonical setup-difftree skill at
  github.com/smorinlabs/difftree-action (which installs the CLI and scaffolds
  the PR workflow) and points at this repo's README for a plain local
  `cargo install difftree`.
---

# setup-difftree

Pointer only: the real difftree install/setup skill lives in the companion
**difftree-action** repo. This routes you there — it installs and configures
nothing itself.

## When this fires

Triggers: "install difftree", "set up difftree", "add difftree to my CI", "add
PR diff-tree comments", "set up difftree-action".

## Where the setup skill lives

The full **`setup-difftree`** skill — installs the difftree CLI and scaffolds
`smorinlabs/difftree-action` into a repository's PR workflow — ships in the
difftree-action repo:

- Repo: <https://github.com/smorinlabs/difftree-action>
- Docs: <https://github.com/smorinlabs/difftree-action/blob/main/docs/skills/setup-difftree.md>

Claude Code and Codex auto-discover that skill when you clone or open the
difftree-action repo; the docs page above also shows how to copy it into your
own setup.

## Just installing the CLI?

For a plain local install, no skill is needed — see this repo's Installation
section (<https://github.com/smorinlabs/difftree#installation>):
`cargo install difftree` (crates.io), build from source, or download a prebuilt
binary from the releases page.

## See also

- difftree-action: <https://github.com/smorinlabs/difftree-action> — the
  canonical setup skill and the GitHub Action it wires in.
