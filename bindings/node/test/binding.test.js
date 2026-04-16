"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const xifty = require("../index.js");

const root = path.resolve(__dirname, "../../..");
const fixture = path.join(root, "fixtures/minimal/happy.jpg");

test("version returns a string", () => {
  assert.equal(typeof xifty.version(), "string");
  assert.ok(xifty.version().length > 0);
});

test("probe returns detected format", () => {
  const output = xifty.probe(fixture);
  assert.equal(output.input.detected_format, "jpeg");
});

test("extract normalized returns expected field", () => {
  const output = xifty.extract(fixture, { view: "normalized" });
  const fields = Object.fromEntries(
    output.normalized.fields.map((field) => [field.field, field]),
  );
  assert.equal(fields["device.make"].value.value, "XIFtyCam");
});

test("invalid view throws", () => {
  assert.throws(() => xifty.extract(fixture, { view: "bad-view" }));
});
