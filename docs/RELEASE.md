# Releasing difftree

difftree publishes to [crates.io](https://crates.io/crates/difftree) through an
automated **release-please** pipeline. Day to day you only write
[Conventional Commits](https://www.conventionalcommits.org/); the pipeline
proposes the version bump, changelog, and tag, and ships the crate.

## The flow

```
push to main ──► release-please.yml ──► opens/updates a "Release PR"
                                         (bumps Cargo.toml + Cargo.lock,
                                          regenerates CHANGELOG.md, updates
                                          .release-please-manifest.json)

   merge the Release PR ──► pushes a vX.Y.Z tag ──► publish.yml ──► crates.io
```

- `fix:` → patch (0.3.0 → 0.3.1); `feat:` → minor (0.3.0 → 0.4.0). While the
  crate is pre-1.0, `bump-minor-pre-major` caps breaking changes (`feat!:` /
  `BREAKING CHANGE:`) at a **minor** bump too — they bump major only once the
  crate reaches 1.0. `chore:`/`docs:`/`ci:`/`test:` do not trigger a release.
  Rules live in [`release-please-config.json`](../release-please-config.json).
- `publish.yml` refuses to ship unless the tag is reachable from `main` and the
  tag version matches `Cargo.toml`, then publishes via crates.io **Trusted
  Publishing** (OIDC) — no long-lived registry token.

## One-time setup

These steps are done once by a maintainer. The pipeline does nothing until they
are complete.

### 1. Install the release-please GitHub App

The shared `*-release-please` GitHub App must be installed on
`smorinlabs/difftree` (Settings → GitHub Apps, or the App's installation page →
add this repo). It is the actor whose tag push triggers `publish.yml` — the
default `GITHUB_TOKEN` cannot trigger downstream workflows.

### 2. Add the App credentials as repo secrets

| Secret | Value |
|---|---|
| `RELEASE_PLEASE_CLIENT_ID` | the App's **Client ID** (`Iv23…`) |
| `RELEASE_PLEASE_PRIVATE_KEY` | the App's full `.pem` private key |

Both go in **Secrets** (not Variables). The `repo-secrets` skill sets these from
1Password; or add them manually under Settings → Secrets and variables →
Actions.

### 3. Configure Trusted Publishing on crates.io

On the crate page → Settings → **Trusted Publishing** → add a GitHub publisher:

| Field | Value |
|---|---|
| Repository owner | `smorinlabs` |
| Repository name | `difftree` |
| Workflow filename | `publish.yml` |
| Environment | `crates-io` |

### 4. Create the `crates-io` GitHub environment

Settings → Environments → **New environment** → `crates-io`. Optionally add a
**required reviewer** so every publish is human-approved — `publish.yml`'s
`publish` job is gated on this environment, so the reviewer is prompted only
after the cheap `verify` checks pass.

## Seeding the first version (0.3.0)

crates.io initially held only a `0.0.0` placeholder. `0.3.0` was published
manually once to seed history, and `.release-please-manifest.json` is set to
`0.3.0` so release-please treats it as already shipped. The next release-worthy
commit produces the first fully automated release.

To repeat a manual publish in an emergency (token in `~/.cargo/credentials.toml`):

```bash
cargo publish --dry-run --locked   # verify packaging
cargo publish --locked             # irreversible
git tag vX.Y.Z && git push origin vX.Y.Z
```

## Troubleshooting

- **Release PR never opens** — normal if there are no `fix:`/`feat:` commits
  since the last release tag.
- **Release PR merges but nothing publishes** — confirm `release-please.yml`
  uses the App token (not `GITHUB_TOKEN`) and that the pushed tag matches
  `publish.yml`'s `v*` pattern.
- **`publish` job fails at auth** — Trusted Publishing (step 3) or the
  `crates-io` environment (step 4) is missing or misconfigured.
