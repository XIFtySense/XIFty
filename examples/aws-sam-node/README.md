# XIFty AWS SAM Node Example

This example is the official first-party starting point for running XIFty in
AWS Lambda with Node.js.

It keeps the story deliberately boring:

- a Node.js Lambda handler using `@xifty/xifty`
- a local Lambda layer assembly path
- an AWS SAM template
- local invocation against checked-in fixtures before any AWS deploy

## What This Example Shows

- how to attach XIFty through a Lambda layer
- how to invoke XIFty from a Lambda handler
- how to work with either:
  - a local file path for development
  - an S3 bucket/key for deployed usage

## Prerequisites

- Node.js 20+
- npm
- AWS SAM CLI

## Quick Start

From this directory:

```bash
npm install
npm run prepare:layer
npm run validate
npm run invoke:fixture
npm run invoke:gps
```

## What The Scripts Do

- `npm run prepare:layer`
  assembles `layer/nodejs/node_modules/@xifty/xifty` using the published npm
  package and trims non-Lambda prebuilds

  Local development still uses the host-supported `@xifty/xifty` package from
  this example’s own `node_modules`. The layer assembly path is for Lambda
  packaging, not for forcing a Linux-only build onto your development machine.

- `npm run build:layer:zip`
  produces a Lambda layer zip at `layer/xifty-node-layer.zip`

- `npm run validate`
  runs `sam validate --template-file template.yaml`

- `npm run invoke:fixture`
  invokes the handler locally against `fixtures/minimal/happy.jpg`

- `npm run invoke:gps`
  invokes the handler locally against `fixtures/minimal/gps.jpg`

## Event Shapes

### Local development

```json
{
  "assetPath": "../../fixtures/minimal/happy.jpg",
  "view": "normalized"
}
```

### S3-backed invocation

```json
{
  "bucket": "your-media-bucket",
  "key": "incoming/example.jpg",
  "view": "normalized"
}
```

When `bucket` and `key` are provided, the handler downloads the object to
`/tmp`, then calls XIFty on the downloaded file path.

## Notes

- This example currently targets Lambda `x86_64`.
- `arm64` should be added only after the package path is actually verified.
- This example is intentionally simple. It is meant to be copied and adapted,
  not treated as a complete production framework.
