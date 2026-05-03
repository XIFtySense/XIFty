# XIFty Schema Policy

XIFty exposes a JSON envelope that is intended to be stable enough for
applications, bindings, and demos to consume over time.

This document defines how that schema is managed.

## Scope

This policy applies to the JSON emitted by:

- CLI `probe`
- CLI `extract`
- FFI JSON output
- WASM JSON output
- language bindings that forward the core JSON envelope

It does not apply to internal Rust structs except where they directly shape the
serialized envelope.

## Source Of Truth

The public contract is defined by three things together:

1. `xifty_core::SCHEMA_VERSION`
2. the checked-in JSON Schema files in `schemas/`
3. contract tests and snapshots that assert real output stays aligned

No single one of these is sufficient on its own.

## Current Schema Artifacts

- `schemas/xifty-probe-0.1.0.schema.json`
- `schemas/xifty-analysis-0.1.0.schema.json`

These intentionally model the envelope and core typed shapes without pretending
that every future normalized field set is fixed forever.

## Compatibility Rules

### Allowed without a schema version bump

These are additive and compatible:

- adding new normalized fields
- adding new raw/interpreted metadata entries
- adding new issue codes
- adding new conflict messages
- adding optional object properties
- adding support for more containers or namespaces

Additive growth is expected in XIFty.

### Requires a schema version bump

These are breaking:

- renaming existing top-level envelope properties
- removing existing envelope properties
- changing the meaning of an existing field
- changing the JSON shape of an existing typed value
- changing an enum string already emitted publicly
- changing `normalized.fields[*].field` names already documented as supported
- making an optional property required in a way that breaks existing consumers

### Strong caution

These may be technically additive but still deserve careful review:

- changing precedence rules that alter normalized winners
- changing timestamp normalization format
- changing confidence or provenance semantics
- changing report behavior in ways that alter downstream assumptions

If behavior changes materially, document it even if the schema version does not
change.

## Versioning Rules

`schema_version` is an output-contract version, not a package version.

That means:

- package releases may advance without changing `schema_version`
- bindings may ship patch/minor releases without changing `schema_version`
- `schema_version` only changes when the JSON contract itself changes

Use semantic versioning for the schema value:

- patch: typo-free but otherwise equivalent schema artifact corrections
- minor: additive schema evolution
- major: breaking schema changes

In practice, XIFty should strongly prefer additive minor evolution.

## Update Checklist

When the public JSON contract changes:

1. update `SCHEMA_VERSION` if the change is schema-relevant
2. update the matching files in `schemas/`
3. update `CAPABILITIES.json` if supported normalized fields changed materially
4. update snapshot/contract tests
5. document the change in `README.md` or `STATE_OF_THE_PROJECT.md` when helpful

## Sidecar-derived Fields

XIFty can fold metadata from co-located vendor sidecar files (e.g. Sony NRT
`<basename>M01.XML`) into the same envelope as embedded metadata. The contract
for those entries is:

- `MetadataEntry::namespace` and `Provenance::namespace` carry the adapter
  name (registered adapters: `sony_nrt`, `subtitles`). Existing namespaces
  keep their identity. The `subtitles` adapter discovers `.srt`/`.vtt`/
  `.ass`/`.ssa` siblings of a primary video and emits flat
  `subtitles.<i>.<field>` entries (`format`, `language`, `cue_count`,
  `duration_seconds`, `path`, `first_cue_text`, plus `style_count` for ASS).
  Adding a new adapter namespace is additive — no `SCHEMA_VERSION` bump.
- `Provenance::container` is the literal string `sidecar` so consumers can
  cheaply distinguish embedded vs sidecar-derived entries.
- `Provenance::path` points at the sidecar file (not the primary asset).
- `NormalizedField::sources[]` carries the same provenance through, so
  callers can audit which fields came from which sidecar.
- Sidecar discovery is **opt-in**: the CLI `--sidecars` flag, the FFI
  `xifty_extract_json_with_options` entrypoint with `enable_sidecars: true`,
  and (in WASM, where it's a no-op) emits an `info`-severity issue
  (`sidecar_discovery_unavailable_in_wasm`).
- Sidecar adapters surface problems through the standard `report.issues`
  channel. The stable issue codes are:
  - `sidecar_target_missing` — adapter referenced a sidecar but the file
    could not be read.
  - `sidecar_no_index_entry` — adapter looked up a primary asset in an
    index sidecar (e.g. MEDIAPRO `mediapro.xml`) and did not find a
    matching entry.
  - `sidecar_unknown_schema_version` — the sidecar's declared schema /
    namespace is not recognized by the adapter; partial parse may still
    proceed.
  - `sidecar_parse_error` — the underlying sidecar file failed to parse
    (e.g. malformed XML); embedded metadata still flows through.
  - `sidecar_discovery_unavailable_in_wasm` — caller asked for sidecar
    discovery on a surface (WASM) with no filesystem access.
- Sidecar additions are *additive only* — no `SCHEMA_VERSION` bump.

## Design Intent

The schema should stay:

- stable at the envelope level
- explicit about typed values
- provenance-preserving
- permissive enough for additive growth
- narrow enough to be trustworthy

XIFty should not freeze immature internal models too early, but it also should
not ask downstream users to reverse-engineer the contract from snapshots alone.
