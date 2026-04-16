# Node Binding

This is the incubating Node package for XIFty.

It is designed to split into its own public repository under the `XIFtySense`
organization once the package shape settles.

Recommended extraction target:

- repo: `XIFtySense/XIFtyNode`
- package: `@xiftysense/xifty-node`

## Architecture

- native layer: `node-addon-api`
- core dependency: `xifty-ffi`
- data exchange: JSON strings from native code, parsed in JavaScript

## Local Development

```bash
cargo build -p xifty-ffi
cd bindings/node
npm install
npm test
```
