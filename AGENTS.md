# AGENTS.md

This file provides working instructions for software agents contributing to XIFty.

## Read First

Before making substantial changes, review:

- [VISION.md](./VISION.md)
- [RESEARCH.md](./RESEARCH.md)
- [ARCHITECTURE_PLAN.md](./ARCHITECTURE_PLAN.md)
- [ENGINEERING_PRINCIPLES.md](./ENGINEERING_PRINCIPLES.md)
- [CONTRIBUTING.md](./CONTRIBUTING.md)

## XIFty Agent Rules

### 1. Protect architectural boundaries

Do not blur these boundaries:

- container parsing
- metadata interpretation
- normalization
- policy
- validation
- serialization
- bindings
- CLI

If a proposed change spans several of these, be explicit about why.

### 2. Optimize for clean code, not maximum code volume

Prefer:

- small modules
- focused functions
- explicit types
- intention-revealing names
- straightforward control flow

Avoid:

- speculative abstraction
- giant utility layers
- framework-style indirection without evidence it is needed
- collapsing unrelated responsibilities into one module

### 3. Preserve provenance

XIFty must keep raw source metadata traceable. Do not normalize away the evidence trail.

### 4. Make ambiguity visible

If a file is malformed, conflicting, partial, or uncertain, surface that in issues, reports, or typed results.

### 5. Keep the ABI narrow

When working near FFI or bindings:

- prefer the C ABI as the stable contract
- avoid exposing internal Rust structures directly
- prefer serialized envelopes over rich cross-language object graphs

### 6. Keep v1 scope disciplined

Do not casually add:

- broad write support
- repair workflows
- wide format expansion outside the current plan
- public APIs that freeze immature internal models

### 7. Refactor when friction reveals design problems

Do not build around messy boundaries if a clean local refactor would improve the design.

### 8. Explain tradeoffs honestly

If you make an assumption, say so.

If something is risky, say so.

If verification is incomplete, say so.

## Preferred Agent Behavior

- Read before editing.
- Make the smallest coherent change that moves the project forward.
- Add or update tests when behavior changes.
- Leave docs better when decisions become clearer.
- Keep future human maintainers in mind at all times.

## Professional Standard

Agents contributing to XIFty are expected to act in the spirit of disciplined software craftsmanship:

- do not knowingly introduce confusing design
- do not represent guesses as conclusions
- do not favor cleverness over clarity
- do not leave hidden messes for the next contributor

When in doubt, choose the clearer design.
