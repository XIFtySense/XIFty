import { test, expect } from "@playwright/test";
import * as path from "node:path";

const REPO_ROOT = path.resolve(__dirname, "../../../..");
const FIXTURE_PATH = path.resolve(REPO_ROOT, "fixtures/minimal/dji_mavic3.mp4");

test("dji_mavic3.mp4 renders the drone cockpit panel", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("pageerror", (err) => consoleErrors.push(String(err)));
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });

  await page.goto("/?smoketest=1");
  await page.setInputFiles("#file-input", FIXTURE_PATH);
  await page.waitForSelector(".cockpit-panel", { state: "visible", timeout: 10_000 });

  // Three instruments: attitude, heading, gimbal.
  expect(await page.locator(".cockpit-instrument").count()).toBe(3);

  // Each instrument has an SVG and a labelled readout.
  expect(await page.locator(".cockpit-instrument .cockpit-svg").count()).toBe(3);

  // Readouts carry the fixture values verbatim.
  const readouts = (await page.locator(".cockpit-readout strong").allTextContents()).join(" | ");
  expect(readouts).toContain("+0.9° pitch");      // flight pitch
  expect(readouts).toContain("−3.9° roll");       // flight roll
  expect(readouts).toContain("+175.5°");          // heading
  expect(readouts).toContain("−31.2° pitch");     // gimbal pitch
  expect(readouts).toContain("−2.3° yaw");        // gimbal yaw

  // Summary strip carries identity + location.
  const summary = await page.locator(".cockpit-summary").innerText();
  expect(summary).toContain("FC3682");
  expect(summary).toContain("53HQN4T0M5B7JW");
  expect(summary).toContain("40.7922");
  expect(summary).toContain("−73.9584");

  // No console errors during render.
  expect(consoleErrors).toEqual([]);
});

test("happy.jpg does not render the cockpit panel (no drone telemetry)", async ({ page }) => {
  const FIXTURE = path.resolve(REPO_ROOT, "fixtures/minimal/happy.jpg");
  await page.goto("/?smoketest=1");
  await page.setInputFiles("#file-input", FIXTURE);
  // Wait for either the cockpit to appear or the structured output to fully render.
  await page.waitForSelector(".structured-output:not([hidden])", { timeout: 10_000 });
  await page.waitForTimeout(200); // settle
  expect(await page.locator(".cockpit-panel").count()).toBe(0);
});
