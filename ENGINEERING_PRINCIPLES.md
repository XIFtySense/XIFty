# XIFty Engineering Principles

XIFty is built by people who care deeply about clean code, clean architecture, disciplined craftsmanship, and professional responsibility.

This document makes those expectations explicit for both human contributors and software agents.

## Our Lineage

XIFty is strongly influenced by:

- Xtreme Programming
- clean code discipline
- clean architecture thinking
- professional programming ethics

We do not treat these as branding or nostalgia. We treat them as working constraints.

## What We Believe

### Clean code matters

We value code that is:

- easy to read
- easy to change
- hard to misuse
- honest about intent
- small where possible
- explicit at boundaries

Code is for humans first. Machines only execute it.

### Architecture matters because change matters

XIFty is expected to grow across:

- multiple formats
- multiple metadata namespaces
- multiple language bindings
- multiple output modes

That means architecture must protect the core from collapse under growth.

We prefer boundaries, stable abstractions, dependency direction, and clear policies over clever shortcuts.

### Professionalism is not optional

We are responsible for the quality of what we ship.

That means:

- we do not knowingly leave the code worse
- we do not hide uncertainty
- we do not hand-wave correctness
- we do not treat tests as optional paperwork
- we do not confuse speed with haste

## XIFty Design Commitments

These are project-level commitments:

- Container parsing and metadata interpretation must remain separate.
- Raw provenance must never be discarded just because normalized output exists.
- Public interfaces must be smaller and more stable than internal implementations.
- Validation and conflict reporting are product features, not incidental byproducts.
- Readability beats local cleverness.
- Simplicity beats premature generality.
- We refactor when design pressure appears instead of building around accumulating mess.

## Rules For Clean Code

### Prefer intention-revealing names

Names should communicate purpose, not implementation trivia.

### Keep functions focused

Functions should do one coherent thing.

If a function needs a long comment to explain its mixed responsibilities, it probably wants to be split.

### Keep modules cohesive

Files and modules should have a clear reason to exist.

### Push complexity to the edges

Parsing hostile binary formats is inherently complex. The surrounding API should not be.

### Avoid hidden coupling

If two modules rely on each other’s internal behavior, the design is not finished.

### Make illegal states difficult

Use types, boundaries, and explicit state modeling to reduce ambiguity and misuse.

### Prefer explicit errors over silent recovery

If we tolerate malformed files, that tolerance should be observable in issues or reports.

### Comment why, not what

Comments should explain intent, invariants, tradeoffs, or constraints.

Comments should not narrate obvious code.

## Rules For Clean Architecture

### Separate policy from detail

Formats, bindings, and serialization are details.

Core metadata concepts, normalization policy, and conflict reasoning are central policy.

### Depend inward

Higher-level policy should not depend on lower-level implementation details.

### Protect the domain

The internal model should not be shaped by CLI convenience, FFI convenience, or external tool quirks.

### Stabilize boundaries

Cross-crate boundaries and the public ABI should be designed conservatively.

### Delay irreversible decisions

We avoid locking the codebase into broad write support, unstable APIs, or over-wide abstractions before the read/normalize core is strong.

## Rules From Xtreme Programming

We want the spirit of XP in how XIFty evolves:

- small increments
- constant feedback
- continuous integration
- relentless refactoring
- shared code ownership
- simple design
- test-first thinking where practical

For XIFty, that means:

- implement one narrow slice end-to-end before broadening
- keep the system always buildable
- add tests before and during expansion of behavior
- compare behavior continuously against real fixtures and trusted tools

## Professional Oath

This is XIFty’s adapted professional oath, inspired by the same spirit as the craftsman’s oath tradition:

- I will not knowingly ship work that I believe is careless or misleading.
- I will tell the truth about what the code does, what it does not do, and what I do not yet know.
- I will leave the system cleaner than I found it when I can do so responsibly.
- I will prefer discipline over ego, clarity over cleverness, and correctness over appearance.
- I will respect future maintainers by writing code they can understand and safely change.
- I will test my work in proportion to the risk it introduces.
- I will surface uncertainty, defects, and tradeoffs instead of burying them.
- I will treat contributors as teammates and the codebase as a shared professional responsibility.

## What This Means In Practice

Before merging work, we should be able to answer yes to most of these:

- Is the design clearer than before?
- Is the code easier to read than the simplest plausible alternative?
- Are boundaries more explicit, not less?
- Are tests and fixtures aligned with the changed behavior?
- Did we avoid unnecessary generalization?
- Did we document important decisions and tradeoffs?
- Would a new contributor understand why this change belongs where it does?

If the answer is no, we keep working.
