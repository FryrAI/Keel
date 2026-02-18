# Release Secrets Setup

The release workflow (`.github/workflows/release.yml`) needs these GitHub repo secrets to fully complete. The GitHub Release + binary builds work without them, but crates.io and Homebrew publishing will fail.

## Required Secrets

### `CARGO_REGISTRY_TOKEN`

Needed for: `cargo publish` to crates.io

**Setup:**
1. Go to https://crates.io/settings/tokens
2. Create a new token with `publish-update` scope
3. Add to repo: `gh secret set CARGO_REGISTRY_TOKEN --repo FryrAI/Keel`

**Note:** First publish requires `cargo publish` to be run manually for each crate in dependency order (keel-core → keel-parsers → keel-enforce → keel-output → keel-server → keel-cli), since crates.io needs each dep to exist before dependents can reference it.

### `HOMEBREW_TAP_TOKEN`

Needed for: pushing updated formula to `FryrAI/homebrew-keel`

**Setup:**
1. Create a fine-grained PAT at https://github.com/settings/personal-access-tokens/new
2. Scope it to the `FryrAI/homebrew-keel` repo with **Contents: Read and write** permission
3. Add to repo: `gh secret set HOMEBREW_TAP_TOKEN --repo FryrAI/Keel`

**Prerequisite:** The `FryrAI/homebrew-keel` repo must exist with a `Formula/` directory. Create it if it doesn't:
```bash
gh repo create FryrAI/homebrew-keel --public --description "Homebrew tap for keel"
```

### Already Set

| Secret | Status | Purpose |
|--------|--------|---------|
| `GIST_TOKEN` | Set 2026-02-17 | Syncs install.sh to public gist |

## Verification

After setting secrets, re-tag to trigger a full release:
```bash
git tag -d v0.1.0
git push origin --delete v0.1.0
git tag -a v0.1.0 -m "keel v0.1.0"
git push origin v0.1.0
```

Then check all jobs pass:
```bash
gh run list --workflow release.yml --limit 1
gh run view <run-id>
```
