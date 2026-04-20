import { test, expect } from "@playwright/test";
import Ajv from "ajv/dist/2020";
import addFormats from "ajv-formats";
import * as fs from "node:fs";
import * as path from "node:path";

// __dirname resolves to demo/web/smoketest/tests/
// Repo root is four levels up.
const REPO_ROOT = path.resolve(__dirname, "../../../..");
const FIXTURE_PATH = path.resolve(REPO_ROOT, "fixtures/minimal/happy.jpg");
const SCHEMA_PATH = path.resolve(
  REPO_ROOT,
  "schemas/xifty-analysis-0.1.0.schema.json",
);

test.beforeAll(() => {
  if (!fs.existsSync(FIXTURE_PATH)) {
    throw new Error(`fixture missing at ${FIXTURE_PATH}`);
  }
  if (!fs.existsSync(SCHEMA_PATH)) {
    throw new Error(`schema missing at ${SCHEMA_PATH}`);
  }
});

test("happy.jpg extracts a schema-valid envelope with expected device make", async ({
  page,
}) => {
  const consoleErrors: string[] = [];
  page.on("pageerror", (err) => consoleErrors.push(String(err)));
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(msg.text());
    }
  });

  await page.goto("/?smoketest=1");

  // Drive the real file input with the fixture.
  await page.setInputFiles("#file-input", FIXTURE_PATH);

  // Wait for the test-only debug hook to populate.
  await page.waitForFunction(() => (window as any).__xiftyDebug != null, null, {
    timeout: 15_000,
  });

  const debugPayload = await page.evaluate(
    () => (window as any).__xiftyDebug,
  );

  expect(debugPayload).toBeTruthy();
  expect(debugPayload.probe).toBeTruthy();
  expect(debugPayload.views).toBeTruthy();

  // Probe shape: detected format should be jpeg.
  expect(debugPayload.probe.input.detected_format).toBe("jpeg");

  // Envelope (the `normalized` view is the full analysis envelope).
  const envelope = debugPayload.views.normalized;
  expect(envelope).toBeTruthy();

  // Known-value assertion mirroring crates/xifty-wasm/src/lib.rs L99-101.
  const fields = envelope.normalized.fields as Array<{
    field: string;
    value: { kind: string; value: unknown };
  }>;
  const deviceMake = fields.find((f) => f.field === "device.make");
  expect(deviceMake).toBeDefined();
  expect(deviceMake!.value.value).toBe("XIFtyCam");

  // Schema validation.
  const schema = JSON.parse(fs.readFileSync(SCHEMA_PATH, "utf8"));
  const ajv = new Ajv({ allErrors: true, strict: false });
  addFormats(ajv);
  const validate = ajv.compile(schema);
  const ok = validate(envelope);
  if (!ok) {
    throw new Error(
      `envelope failed schema validation: ${JSON.stringify(validate.errors, null, 2)}`,
    );
  }

  expect(consoleErrors, `browser console errors: ${consoleErrors.join("\n")}`).toEqual([]);
});
