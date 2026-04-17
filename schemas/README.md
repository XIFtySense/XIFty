# XIFty Schemas

This directory contains checked-in JSON Schema artifacts for XIFty's public JSON
envelopes.

Current artifacts:

- `xifty-probe-0.1.0.schema.json`
- `xifty-analysis-0.1.0.schema.json`

These schema files are intended to support:

- downstream consumers that want a machine-readable contract
- future schema evolution with explicit versioning
- contract checks in tests and release workflows

They are deliberately conservative:

- the top-level envelope shape is explicit
- the typed value model is explicit
- additive growth is allowed where XIFty is expected to expand

The governing rules for how these files evolve live in
[docs/SCHEMA_POLICY.md](../docs/SCHEMA_POLICY.md).
