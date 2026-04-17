"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { handler } = require("../src/handler");

async function main() {
  const eventFile = process.argv[2];
  if (!eventFile) {
    throw new Error("usage: node scripts/invoke-local.cjs <event-json>");
  }

  const eventPath = path.resolve(eventFile);
  const raw = fs.readFileSync(eventPath, "utf8");
  const event = JSON.parse(raw);
  const result = await handler(event);
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
}

main().catch((error) => {
  console.error(error.stack || String(error));
  process.exitCode = 1;
});
