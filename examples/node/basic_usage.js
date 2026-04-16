"use strict";

const path = require("node:path");
const xifty = require("../../bindings/node");

const fixture = path.resolve(__dirname, "../../fixtures/minimal/happy.jpg");

console.log("XIFty version:", xifty.version());
console.log(JSON.stringify(xifty.extract(fixture, { view: "normalized" }), null, 2));
