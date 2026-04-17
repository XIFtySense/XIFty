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
4. tag from the exact commit that contains the intended fix
5. publish release notes that describe the shipped surface, not just merged code

## Node Package Release

Before publishing `@xifty/xifty`:

1. build the release prebuilds

```bash
cd /Users/k/Projects/XIFtyNode
npm run build:prebuilds
```

2. verify packaged contents

```bash
npm run verify:package
npm run verify:tarball
```

3. if a local real-world fixture exists, smoke-test the packed tarball against it

Example for the Sony XAVC regression path:

```bash
XIFTY_SMOKE_FIXTURE=/Users/k/Projects/XIFty/fixtures/local/C0242.MP4 \
XIFTY_SMOKE_FIELDS=video.bitrate,audio.sample_rate \
XIFTY_SMOKE_NONZERO_FIELDS=video.bitrate,audio.sample_rate \
npm run verify:tarball
```

4. publish only after the tarball, not just the repo checkout, behaves correctly
5. verify the registry after publish

```bash
npm view @xifty/xifty version dist-tags.latest --json
```

## Closing Customer Issues

Do not close issues based only on:

- passing local repo tests
- a merged commit
- a release tag existing

Close only after the shipped artifact that customers consume has been verified.
