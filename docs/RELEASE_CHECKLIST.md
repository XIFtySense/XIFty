# XIFty Release Checklist

This checklist exists to keep repo correctness and shipped-artifact correctness
in sync.

## Core Release

Before cutting a core release:

1. verify workspace behavior

```bash
cargo test --workspace
```

2. verify checked-in schema examples against the public schemas

```bash
python3 -m pip install jsonschema
python3 tools/validate_schema_examples.py
```

3. confirm `schemas/`, `CAPABILITIES.json`, and `SCHEMA_VERSION` still agree
4. bump the workspace version, refresh `Cargo.lock`, commit, tag, push:

```bash
# in this repo, on main, after the feature PR has merged
sed -i '' 's/^version = "0.1.X"/version = "0.1.Y"/' Cargo.toml   # workspace.package
cargo build --workspace                                          # refreshes Cargo.lock
git add Cargo.toml Cargo.lock
git commit -m "Release core 0.1.Y"
git push
git tag -a v0.1.Y -m "Release core 0.1.Y"
git push origin v0.1.Y
```

5. create a GitHub Release for the tag (`gh release create v0.1.Y --notes …`)
   — this triggers `runtime-artifacts.yml`, which builds and uploads
   `xifty-runtime-{macos-arm64,linux-x64}-v0.1.Y.tar.gz` assets.
6. confirm both runtime artifacts landed on the release before announcing

```bash
gh release view v0.1.Y --json assets --jq '.assets[] | "\(.size)\t\(.name)"'
```

7. publish release notes that describe the shipped surface, not just merged code

## Node Package Release

The Node binding has its own deep-dive in
[`XIFtyNode/RELEASING.md`](https://github.com/XIFtySense/XIFtyNode/blob/main/RELEASING.md).
The checklist below is the short version. **Read the deep-dive first if you
have not published from this machine recently** — npm's 2FA / token /
package-access rules have changed multiple times and the failure modes are
non-obvious.

### Pre-flight (one-time per machine, but verify each release)

1. **`npm whoami`** returns `xifty`. If 401, run `npm login --auth-type=web`.
2. **Package publishing-access mode** at
   https://www.npmjs.com/package/@xifty/xifty/access is set to
   *"Require two-factor authentication or a granular access token with
   bypass 2fa enabled"* — **not** the *"… and disallow tokens"* option.
   If you flip the radio you must confirm with your enrolled passkey in
   the browser; the CLI cannot change this setting without an
   authenticator-app TOTP.
3. **`~/.npmrc` carries a granular access token with *Bypass 2FA*
   checked**, scoped to `@xifty/xifty` with read-and-write. Tokens
   without the bypass flag will get `EOTP` even though `npm whoami`
   succeeds. Generate at https://www.npmjs.com/settings/~/tokens →
   *Generate New Token* → *Granular Access Token*.
4. **Docker is running** — the Linux-x64 prebuild is built in an Amazon
   Linux 2023 container; without Docker the bundle ships only the macOS
   prebuild.

### Publish

```bash
cd /Users/k/Projects/XIFtyNode    # or wherever your XIFtyNode checkout lives
npm version patch                 # 0.1.7 → 0.1.8 (auto-commits + tags)
git push --follow-tags
XIFTY_CORE_REF=v0.1.Y npm run publish:local   # pin to the core tag from above
```

`publish:local` runs prebuilds → `verify:package` → `verify:tarball` →
`verify:linux-x64` → `npm publish`. Plan for ~5 min on a warm cache.

### Post-publish

```bash
npm view @xifty/xifty version dist-tags.latest --json
```

If a local real-world fixture exists, smoke-test the *packed tarball*
(not the repo checkout) against it:

```bash
XIFTY_SMOKE_FIXTURE=/Users/k/Projects/XIFty/fixtures/local/C0242.MP4 \
XIFTY_SMOKE_FIELDS=video.bitrate,audio.sample_rate \
XIFTY_SMOKE_NONZERO_FIELDS=video.bitrate,audio.sample_rate \
npm run verify:tarball
```

**Revoke the bypass-2fa publish token** at
https://www.npmjs.com/settings/~/tokens after the release lands, unless
you have another release coming up in the same window. These tokens can
publish silently — treat them like a deploy key.

### Common failure modes

| Error | Cause |
|---|---|
| `EOTP — This operation requires a one-time password from your authenticator.` | Token missing *Bypass 2FA* flag, **or** package access mode is `mfa=publish` (disallow tokens). Both must be aligned. |
| `403 Two-factor authentication is required to publish this package but an automation token was specified.` | Token is correct (bypass-2fa), but the package access setting is still `mfa=publish`. Flip the radio in the browser. |
| `EPUBLISHCONFLICT` | Version already published. Bump again — npm versions are immutable. |

## Branch Protection

`main` enforces the following required status checks before a PR can merge
(configured under Settings → Branches → Branch protection rules):

- `Rust Core` (from `ci.yml`)
- `Runtime Artifact (macos-arm64)` (from `ci.yml`)
- `Runtime Artifact (linux-x64)` (from `ci.yml`)
- `Lambda Node Example` (from `ci.yml`)
- `Docs And Contract` (from `hygiene.yml`) — enforces the cbindgen header
  staleness check and JSON schema artifact validation on every PR

"Require branches to be up to date before merging" is also enabled, so PRs
must rebase onto the latest `main` before the merge button unlocks.

Verify the rule has not drifted:

```bash
gh api repos/XIFtySense/XIFty/branches/main/protection \
  --jq '.required_status_checks.contexts'
```

Expected output (order may vary):

```json
["Rust Core","Runtime Artifact (macos-arm64)","Runtime Artifact (linux-x64)","Lambda Node Example","Docs And Contract"]
```

If the repository migrates to Rulesets, the classic `branches/*/protection`
endpoint returns empty even when rules are active. Cross-check with:

```bash
gh api repos/XIFtySense/XIFty/rulesets
```

`Oracle Differentials` (from `hygiene.yml`) is intentionally not a required
check: it runs ExifTool on the weekly schedule and on manual dispatch only.

## Closing Customer Issues

Do not close issues based only on:

- passing local repo tests
- a merged commit
- a release tag existing

Close only after the shipped artifact that customers consume has been verified.
