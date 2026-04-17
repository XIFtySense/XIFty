# XIFty Iteration Ten Plan

## Summary

Iteration ten should focus on the fastest credible production-adoption path for
teams like kstore:

- an official AWS Lambda story for the Node binding
- a simple AWS SAM example that works out of the box
- a clear split between serverless-native adoption and the newer WASM/browser
  story

This iteration is not about broad new metadata capabilities. It is about making
XIFty easier to deploy in the environments customers already run today.

The intended outcome is:

- a documented and tested Lambda packaging strategy for `@xifty/xifty`
- an official AWS SAM example/template
- a repeatable way to publish or assemble a Lambda layer for supported Node
  runtimes
- adoption docs that make XIFty feel production-ready instead of experimental

## Why This Iteration

XIFty has now proven three things clearly:

- the core extraction architecture is strong
- the Node binding can ship real native prebuilds
- the browser demo and WASM path make the project easier to evaluate publicly

What is still missing is an obvious production handoff for the first customer.

For kstore, AWS Lambda is a priority. That means XIFty should not require a
team to reverse-engineer:

- how to package native bindings for Lambda
- how to structure the deployment
- how to wire XIFty into a normal event-driven media workflow

The next highest-value move is therefore not another parser expansion. It is
removing serverless adoption friction.

## External Constraints

### Lambda runtime direction

AWS currently supports both `nodejs22.x` and `nodejs24.x` managed runtimes for
Lambda, both on Amazon Linux 2023.

Sources:

- [Building Lambda functions with Node.js](https://docs.aws.amazon.com/lambda/latest/dg/lambda-nodejs.html)
- [Lambda runtimes](https://docs.aws.amazon.com/lambda/latest/dg/lambda-runtimes.html)

### Layer support and size constraints

Lambda layers are a valid deployment primitive for Node.js functions, but the
limits matter:

- up to 5 layers per function
- total unzipped function + layers must stay within 250 MB

Sources:

- [Adding layers to functions](https://docs.aws.amazon.com/lambda/latest/dg/adding-layers.html)
- [Lambda quotas](https://docs.aws.amazon.com/lambda/latest/dg/gettingstarted-limits.html)

### SAM support

AWS SAM supports both:

- `AWS::Serverless::Function`
- `AWS::Serverless::LayerVersion`

This makes SAM the right place for the first official “copy this and ship it”
example.

Sources:

- [Using AWS SAM with layers](https://docs.aws.amazon.com/lambda/latest/dg/layers-sam.html)
- [Building Lambda layers in AWS SAM](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/building-layers.html)

## Primary Goal

Deliver an official XIFty serverless adoption path that is boring, documented,
and easy to copy:

- Node Lambda handler example
- Lambda layer packaging for the Node binding
- AWS SAM template/example
- docs that explain when to use native Node-on-Lambda vs WASM-in-browser

## Scope

### In scope

- a Lambda-focused example for `@xifty/xifty`
- a SAM template that wires a function and layer together
- packaging scripts and/or release artifacts for a Lambda layer
- Node runtime compatibility guidance for Lambda
- architecture and platform support docs
- local and CI validation for the example path

### Out of scope

- broad new metadata extraction scope
- a full AWS CDK construct library
- Terraform modules
- broad multi-cloud serverless packaging
- making WASM the primary Lambda runtime path
- trying to unify every binding around one serverless artifact in this
  iteration

## Product Shape

The public outcome should answer a simple customer question:

"How do I use XIFty in Lambda without fighting native packaging?"

The official answer should be:

1. use the Node binding
2. either attach the official layer or follow the documented layer build path
3. start from the provided SAM example

## Architectural Direction

### Keep the runtime split honest

We now have two distinct deployment stories:

- **browser / edge evaluation path**: WASM
- **serverless Node production path**: native Node binding on Lambda

Do not blur these together.

The docs and examples should make the distinction explicit:

- WASM is ideal for browser demos, local inspection, and some edge contexts
- Node Lambda is the primary production path for AWS serverless ingestion today

### Layer as a packaging aid, not a second API

The Lambda layer must not become a new XIFty API surface. It is only a
deployment package boundary around the existing Node binding.

That means:

- keep the public package API the same
- keep the Lambda layer thin
- avoid introducing Lambda-specific semantics into the binding itself

### SAM example ownership

Recommended location:

- `examples/aws-sam-node/`

That keeps the example close to the core project and makes it easier to evolve
with the official Lambda docs.

## Deliverables

### 1. Official AWS SAM example

Add an example application that includes:

- a minimal Lambda handler using `@xifty/xifty`
- a function that can inspect an input file and return XIFty output
- optional S3 event wiring if it stays simple
- a SAM template that can be built and deployed with clear instructions

The example should prefer simplicity over realism-by-default.

The minimum useful example is:

- direct invocation
- file path or S3 object input
- JSON result output

### 2. Lambda layer packaging path

Define a supported way to assemble or consume an XIFty Node layer.

The iteration should decide one honest path:

- publish a reusable layer artifact, or
- document and automate local layer assembly per release

Recommended first step:

- automate local/release layer assembly in-repo
- defer cross-account public layer publishing if that slows the iteration down

The layer should target:

- `nodejs22.x`
- `nodejs24.x`
- `x86_64`
- `arm64` if feasible in the same release path

If `arm64` is not ready, say so explicitly rather than implying parity.

### 3. Adoption docs

Add documentation that answers:

- when to use XIFty via Node Lambda
- when to use WASM instead
- how to use the SAM example
- how to attach or build the layer
- what runtime and architecture combinations are supported

The docs should feel like customer onboarding, not maintainer notes.

## Build And Release Direction

### Layer assembly

Add a reproducible build path that outputs a Lambda-ready layer zip.

Recommended characteristics:

- based on the existing Node binding package structure
- no reliance on sibling `../XIFty` assumptions
- explicit runtime/architecture targeting
- clear artifact naming

### Validation

The Lambda example should be verifiable without a full AWS deploy for every
change.

Recommended validation layers:

- local Node handler execution
- SAM template validation
- layer artifact assembly validation
- optional containerized Lambda-like smoke tests

If we need AWS-hosted verification later, that can be a follow-on workflow.

## Testing And Verification

### Local verification

- Node binding tests still pass
- Lambda handler example runs locally
- layer package assembles successfully
- SAM template validates cleanly

### CI verification

- example files lint/build/test cleanly
- layer assembly is exercised in CI
- broken packaging should fail before release

### Acceptance criteria

Iteration ten should be considered successful when:

- kstore can start from an official SAM example instead of inventing their own
  packaging
- XIFty provides a documented Lambda layer path for the Node binding
- runtime support boundaries are explicit
- the Node Lambda path is clearly positioned as the serverless production story
- the WASM demo path remains clearly positioned as browser/edge evaluation

## Risks And Tradeoffs

### Risk: layer complexity without enough benefit

If the layer path becomes more complex than bundling the package directly into a
function deployment, the docs should say so honestly.

The layer should exist only if it reduces friction for repeated adoption.

### Risk: architecture claims outrun validation

Do not claim full `arm64` parity unless it is actually built and tested.

### Risk: drift between package and example

The example must stay close enough to the real binding/release path that it
does not become stale marketing code.

## Assumptions And Defaults

- AWS Lambda is the highest-priority serverless production target right now.
- The Node binding is the correct primary Lambda integration surface.
- WASM remains important, but as a browser/edge path rather than the primary
  AWS Lambda path.
- The first customer value is deployment simplicity, not configuration
  flexibility.
- SAM is the right official example format for this iteration.
