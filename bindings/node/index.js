"use strict";

const native = require("./build/Release/xifty_node.node");

const viewByName = {
  full: 0,
  raw: 1,
  interpreted: 2,
  normalized: 3,
  report: 4,
};

function version() {
  return native.version();
}

function probe(path) {
  return JSON.parse(native.probeJson(path));
}

function extract(path, options = {}) {
  const view = options.view ?? "full";
  const viewMode = typeof view === "number" ? view : viewByName[String(view)];
  if (viewMode === undefined) {
    throw new TypeError(`unsupported view: ${view}`);
  }
  return JSON.parse(native.extractJson(path, viewMode));
}

module.exports = {
  version,
  probe,
  extract,
};
