## Context

Reviewer finding from PR #127 (Phase 1 sidecar framework + Sony NRT adapter, #122).

## Problem

The synthetic-fixture integration test \`sony_nrt_sidecar_synthetic_minimal_fixture_lifts_fields\` in \`crates/xifty-cli/tests/cli_contract.rs\` asserts 9 of the 12 normalized fields lifted by \`derive_sony_nrt_fields\`. Three are missing:

- \`timecode.half_step\`
- \`recording.cache_rec\`
- \`device.serial_no\`

All three are present in the synthetic fixture XML and are correctly lifted at the unit-test level (\`lifts_sony_nrt_sidecar_fields_from_namespace\` in \`xifty-normalize\` covers all 12). The integration test is the load-bearing one for the cross-crate pipeline; coverage gap means a regression in the CLI dispatch path or normalize wiring for any of these three fields wouldn't be caught by the integration test alone.

## Fix

Add the three missing assertions to the integration test. Single-line additions per field; trivial.

## Acceptance criteria

- All 12 normalized fields lifted by Phase 1 are asserted by the integration test.
- Test still skips cleanly if the synthetic fixture has not been generated.
- Validation per \`.loswf/config.yaml\` passes.
