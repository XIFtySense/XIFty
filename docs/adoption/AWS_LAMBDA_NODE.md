# XIFty on AWS Lambda (Node.js)

This is the official first-party serverless adoption path for XIFty today.

Use this path when:

- you want to run XIFty inside AWS Lambda
- you already have Node.js Lambda functions
- you want the most boring production path for server-side media inspection

Do not use the browser/WASM demo path as your primary Lambda integration.
WASM is the right fit for browser and edge evaluation. For AWS Lambda, the Node
binding is the intended production surface.

## What We Recommend

Start from the example in:

- [examples/aws-sam-node](/Users/k/Projects/XIFty/examples/aws-sam-node)

That example gives you:

- a Lambda handler using `@xifty/xifty`
- a SAM template
- a reproducible local layer build path
- a local invocation flow for checked-in fixtures

## Supported Runtime Direction

Current intended Lambda runtime targets:

- `nodejs22.x`
- `nodejs24.x`

Current intended architecture target:

- `x86_64`

`arm64` is not claimed here yet. Add it only after it is actually built and
verified.

## Layer Strategy

The layer strategy is intentionally simple:

- install the published `@xifty/xifty` package into the standard Lambda
  `nodejs/node_modules` layout
- trim non-Linux prebuilds so the layer reflects the Lambda target
- attach the layer to your function with SAM

The layer is a packaging aid, not a separate XIFty API.

## Local Workflow

From the example directory:

```bash
npm install
npm run prepare:layer
npm run validate
npm run invoke:fixture
npm run invoke:gps
```

That validates:

- the handler shape
- the layer assembly path
- the SAM template
- the XIFty extraction result against local fixture files

## Why Node Lambda Instead Of WASM

Choose Node Lambda when you want:

- the simplest production AWS path
- the published Node package
- predictable use inside existing Lambda handlers

Choose WASM when you want:

- browser-side local inspection
- static demos
- some edge/browser-style runtimes

## Notes

- The total unzipped size of the function plus layers must stay within Lambda’s
  unzipped deployment limit.
- Lambda layers are useful when multiple functions need the same XIFty package.
- If a single function owns XIFty alone, bundling directly into the function may
  also be a reasonable choice.
