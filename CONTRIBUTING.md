# Contributing To XIFty

Thank you for contributing to XIFty.

This project cares as much about how we build as what we build. Please read [ENGINEERING_PRINCIPLES.md](./ENGINEERING_PRINCIPLES.md) before making substantial changes.

## Who This Is For

This guide applies to:

- maintainers
- external contributors
- contract contributors
- software agents acting on behalf of contributors

## Contribution Standard

We accept contributions that improve the system without degrading its clarity.

That means a change is not complete just because it works. It should also:

- fit the architecture
- improve or preserve readability
- include appropriate tests or fixtures
- make boundaries clearer, not blurrier
- avoid unnecessary abstraction

## Expected Workflow

### Start from a narrow goal

Prefer small, coherent changes over broad speculative refactors.

### Understand the boundary you are touching

Before editing code, identify whether the change belongs in:

- container parsing
- metadata interpretation
- normalization
- policy
- validation
- serialization
- bindings
- CLI

If the answer is not clear, the design likely needs thought before code.

### Make the smallest clean change that solves the problem

Do not expand scope because you noticed something adjacent.

### Refactor when needed

If the existing design resists a clean implementation, improve the design rather than piling on conditionals and workarounds.

## Coding Expectations

### Prefer readability over cleverness

Write the version a strong teammate can understand quickly.

### Avoid mixed responsibilities

Do not let one module become parser, policy engine, serializer, and fixer at the same time.

### Preserve provenance

Do not collapse raw source metadata into normalized fields without retaining traceability.

### Surface ambiguity

Conflicts, malformed inputs, and weak interpretations should appear in reports or issues, not disappear silently.

### Keep interfaces narrow

Especially for:

- public Rust APIs
- JSON output envelopes
- C ABI surfaces
- binding wrappers

## Tests And Verification

The required level of verification depends on risk, but behavior changes should usually include one or more of:

- unit tests
- fixture-based tests
- snapshot tests
- differential comparisons
- fuzz target updates

At minimum, contributors should explain what they verified.

## Commit And Review Guidance

Good contributions make review easier.

Please aim for:

- small commits with coherent intent
- clear PR descriptions
- direct explanation of tradeoffs
- explicit mention of remaining risks or uncertainty

## Branch And Pull Request Expectations

For the public-facing repository, contributors should assume this workflow:

- open a focused branch for each coherent change
- open a pull request before merging
- keep pull requests small enough to review thoughtfully
- link the change to the relevant docs, fixtures, or capability claims when
  those are affected

Pull requests should make it easy for a reviewer to answer:

- what changed?
- why does it belong in XIFty?
- what was verified?
- what remains intentionally out of scope?

If a change affects supported behavior, please call out any impact on:

- CLI output
- `CAPABILITIES.json`
- fixtures or differential coverage
- FFI or external package expectations

## What We Will Push Back On

We will usually reject or ask to revise changes that:

- add abstraction before it is needed
- hide complexity instead of isolating it
- cross architectural boundaries casually
- weaken provenance or validation behavior
- add broad write behavior before the read core is mature
- optimize prematurely without measurement
- make the system harder to reason about

## Guidance For Agent Contributors

Agents contributing to XIFty should:

- read project docs before proposing structure
- preserve the architectural separation already established
- avoid inventing broad frameworks or scaffolding without clear need
- prefer explicit, typed, local changes
- document assumptions clearly
- never represent uncertainty as fact

Agents should optimize for maintainability by humans who did not participate in the change.

## If You Are Unsure

When in doubt:

- choose the simpler design
- preserve existing boundaries
- make uncertainty visible
- ask for architectural clarification before broadening scope
