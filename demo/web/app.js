import init, { extract_bytes, probe_bytes } from "./pkg/xifty_wasm.js";

const fileInput = document.getElementById("file-input");
const dropZone = document.getElementById("drop-zone");
const summaryPanel = document.getElementById("summary-panel");
const resultsPanel = document.getElementById("results-panel");
const errorPanel = document.getElementById("error-panel");
const structuredOutput = document.getElementById("structured-output");
const resultOutput = document.getElementById("result-output");
const statusText = document.getElementById("status-text");
const errorText = document.getElementById("error-text");
const copyJsonButton = document.getElementById("copy-json");
const tabs = Array.from(document.querySelectorAll(".tab"));

const fileNameEl = document.getElementById("file-name");
const fileSizeEl = document.getElementById("file-size");
const fileFormatEl = document.getElementById("file-format-badge");
const fileContainerEl = document.getElementById("file-container");
const fileIssuesEl = document.getElementById("file-issues");

let currentFile = null;
let currentViews = {};
let currentView = "normalized";

await init();

fileInput.addEventListener("change", async (event) => {
  const [file] = event.target.files ?? [];
  if (file) {
    await handleFile(file);
  }
});

dropZone.addEventListener("dragover", (event) => {
  event.preventDefault();
  dropZone.classList.add("is-dragging");
});

dropZone.addEventListener("dragleave", () => {
  dropZone.classList.remove("is-dragging");
});

dropZone.addEventListener("drop", async (event) => {
  event.preventDefault();
  dropZone.classList.remove("is-dragging");
  const [file] = event.dataTransfer?.files ?? [];
  if (!file) {
    return;
  }
  fileInput.files = event.dataTransfer.files;
  await handleFile(file);
});

tabs.forEach((tab) => {
  tab.addEventListener("click", () => {
    currentView = tab.dataset.view;
    updateTabs();
    renderCurrentView();
  });
});

copyJsonButton.addEventListener("click", async () => {
  const payload = currentViews[currentView];
  if (!payload) {
    return;
  }
  await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
  statusText.textContent = `Copied ${currentView} JSON`;
});

async function handleFile(file) {
  currentFile = file;
  hideError();
  statusText.textContent = "Reading file…";

  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const probe = JSON.parse(probe_bytes(bytes, file.name));
    const views = {};

    for (const view of ["normalized", "interpreted", "raw", "report"]) {
      views[view] = JSON.parse(extract_bytes(bytes, file.name, view));
    }

    currentViews = views;
    populateSummary(file, probe);
    currentView = "normalized";
    updateTabs();
    renderCurrentView();
    resultsPanel.hidden = false;
    statusText.textContent = "Local extraction complete";
  } catch (error) {
    showError(error);
  }
}

function populateSummary(file, probe) {
  summaryPanel.hidden = false;
  fileNameEl.textContent = file.name;
  fileSizeEl.textContent = formatBytes(file.size);
  fileFormatEl.textContent = probe.input.detected_format ?? "unknown";
  fileContainerEl.textContent = probe.input.container ?? "unknown";
  fileIssuesEl.textContent = String(currentViews.report?.report?.issues?.length ?? 0);
}

function renderCurrentView() {
  const payload = currentViews[currentView];
  if (!payload) {
    structuredOutput.hidden = true;
    resultOutput.hidden = false;
    resultOutput.textContent = "";
    return;
  }

  if (currentView === "normalized") {
    renderNormalized(payload);
    return;
  }

  if (currentView === "report") {
    renderReport(payload);
    return;
  }

  structuredOutput.hidden = true;
  resultOutput.hidden = false;
  resultOutput.textContent = JSON.stringify(payload, null, 2);
}

function renderNormalized(payload) {
  const fields = Object.fromEntries(
    (payload.normalized?.fields ?? []).map((field) => [field.field, field]),
  );
  const groups = [
    {
      title: "Core facts",
      entries: [
        labelEntry("Device", combineValues(fields, ["device.make", "device.model"])),
        labelEntry("Captured", fieldValue(fields["captured_at"])),
        labelEntry(
          "Dimensions",
          combineValues(fields, ["dimensions.width", "dimensions.height"], " × "),
        ),
        labelEntry("Orientation", fieldValue(fields["orientation"])),
        labelEntry("Software", fieldValue(fields["software"])),
      ],
    },
    {
      title: "Exposure facts",
      entries: [
        labelEntry("ISO", fieldValue(fields["exposure.iso"])),
        labelEntry("Aperture", formatAperture(fields["exposure.aperture"])),
        labelEntry("Shutter", formatShutter(fields["exposure.shutter_speed"])),
        labelEntry(
          "Focal length",
          appendUnit(fieldValue(fields["exposure.focal_length_mm"]), "mm"),
        ),
        labelEntry("Lens", combineValues(fields, ["lens.make", "lens.model"])),
      ],
    },
    {
      title: "Media facts",
      entries: [
        labelEntry("Duration", appendUnit(fieldValue(fields["duration"]), "s")),
        labelEntry("Video codec", fieldValue(fields["codec.video"])),
        labelEntry("Audio codec", fieldValue(fields["codec.audio"])),
        labelEntry(
          "Frame rate",
          appendUnit(fieldValue(fields["video.framerate"]), "fps"),
        ),
        labelEntry(
          "Bitrate",
          appendUnit(formatInteger(fieldValue(fields["video.bitrate"])), "bps"),
        ),
        labelEntry("Channels", fieldValue(fields["audio.channels"])),
        labelEntry(
          "Sample rate",
          appendUnit(formatInteger(fieldValue(fields["audio.sample_rate"])), "Hz"),
        ),
      ],
    },
  ]
    .map((group) => ({
      ...group,
      entries: group.entries.filter((entry) => entry.value && entry.value !== "—"),
    }))
    .filter((group) => group.entries.length > 0);

  const issueCount = currentViews.report?.report?.issues?.length ?? 0;
  const conflictCount = currentViews.report?.report?.conflicts?.length ?? 0;
  const summaryFacts = [
    { label: "Issues", value: String(issueCount) },
    { label: "Conflicts", value: String(conflictCount) },
    { label: "Format", value: payload.input?.detected_format ?? "—" },
    { label: "Container", value: payload.input?.container ?? "—" },
  ];

  structuredOutput.innerHTML = `
    <section class="nutrition-card" aria-label="Normalized metadata label">
      <header class="nutrition-header">
        <p class="label-kicker">XIFty</p>
        <h3>Metadata facts</h3>
        <p class="label-caption">Normalized application-facing fields from local browser extraction</p>
      </header>
      <div class="nutrition-summary">
        ${summaryFacts
          .map(
            (fact) => `
              <div class="nutrition-summary-item">
                <span>${escapeHtml(fact.label)}</span>
                <strong>${escapeHtml(fact.value)}</strong>
              </div>`,
          )
          .join("")}
      </div>
      ${groups
        .map(
          (group) => `
            <section class="nutrition-group">
              <h4>${escapeHtml(group.title)}</h4>
              ${group.entries
                .map(
                  (entry) => `
                    <div class="nutrition-row">
                      <span>${escapeHtml(entry.label)}</span>
                      <strong>${escapeHtml(entry.value)}</strong>
                    </div>`,
                )
                .join("")}
            </section>`,
        )
        .join("")}
    </section>
  `;

  structuredOutput.hidden = false;
  resultOutput.hidden = true;
}

function renderReport(payload) {
  const issues = payload.report?.issues ?? [];
  const conflicts = payload.report?.conflicts ?? [];

  structuredOutput.innerHTML = `
    <section class="report-layout">
      <div class="report-column">
        <div class="report-head">
          <p class="label-kicker">Report</p>
          <h3>Issues</h3>
        </div>
        ${
          issues.length
            ? issues
                .map(
                  (issue) => `
                    <article class="report-card severity-${escapeHtml(issue.severity ?? "warning")}">
                      <div class="report-card-head">
                        <strong>${escapeHtml(issue.code ?? "issue")}</strong>
                        <span>${escapeHtml(issue.severity ?? "warning")}</span>
                      </div>
                      <p>${escapeHtml(issue.message ?? "No message")}</p>
                    </article>`,
                )
                .join("")
            : '<p class="report-empty">No issues reported.</p>'
        }
      </div>
      <div class="report-column">
        <div class="report-head">
          <p class="label-kicker">Report</p>
          <h3>Conflicts</h3>
        </div>
        ${
          conflicts.length
            ? conflicts
                .map(
                  (conflict) => `
                    <article class="report-card severity-info">
                      <div class="report-card-head">
                        <strong>${escapeHtml(conflict.field ?? "conflict")}</strong>
                        <span>conflict</span>
                      </div>
                      <p>${escapeHtml(conflict.message ?? "No message")}</p>
                    </article>`,
                )
                .join("")
            : '<p class="report-empty">No conflicts reported.</p>'
        }
      </div>
    </section>
  `;

  structuredOutput.hidden = false;
  resultOutput.hidden = true;
}

function updateTabs() {
  tabs.forEach((tab) => {
    tab.classList.toggle("is-active", tab.dataset.view === currentView);
  });
}

function showError(error) {
  errorPanel.hidden = false;
  resultsPanel.hidden = true;
  summaryPanel.hidden = Boolean(currentFile);
  errorText.textContent = error instanceof Error ? error.message : String(error);
  statusText.textContent = "Extraction failed";
}

function hideError() {
  errorPanel.hidden = true;
  errorText.textContent = "";
}

function labelEntry(label, value) {
  return { label, value: value || "—" };
}

function combineValues(fields, keys, separator = " ") {
  const values = keys.map((key) => fieldValue(fields[key])).filter(Boolean);
  return values.length ? values.join(separator) : null;
}

function fieldValue(field, prefix = "") {
  if (!field?.value) {
    return null;
  }
  return prefix + typedValueToString(field.value);
}

function formatShutter(field) {
  if (!field?.value) {
    return null;
  }
  const value = field.value;
  if (value.kind === "rational") {
    const numerator = value.value?.numerator;
    const denominator = value.value?.denominator;
    if (numerator && denominator) {
      if (numerator === 1) {
        return `1/${denominator}s`;
      }
      return `${numerator}/${denominator}s`;
    }
  }
  return `${typedValueToString(value)}s`;
}

function formatAperture(field) {
  if (!field?.value) {
    return null;
  }
  const value = field.value;
  if (value.kind === "rational") {
    const numerator = value.value?.numerator;
    const denominator = value.value?.denominator;
    if (numerator && denominator) {
      const formatted = numerator / denominator;
      if (Number.isFinite(formatted)) {
        return `f/${formatted.toFixed(1).replace(/\\.0$/, "")}`;
      }
    }
  }
  return `f/${typedValueToString(value)}`;
}

function formatInteger(value) {
  if (!value) {
    return null;
  }
  const numeric = Number(value);
  if (Number.isFinite(numeric)) {
    return Intl.NumberFormat("en-US").format(numeric);
  }
  return value;
}

function appendUnit(value, unit) {
  return value ? `${value} ${unit}` : null;
}

function typedValueToString(value) {
  switch (value.kind) {
    case "string":
    case "timestamp":
      return value.value;
    case "integer":
    case "float":
      return String(value.value);
    case "rational":
      return `${value.value?.numerator}/${value.value?.denominator}`;
    case "coordinates":
      return `${value.value?.latitude}, ${value.value?.longitude}`;
    case "dimensions":
      return `${value.value?.width} × ${value.value?.height}`;
    default:
      return JSON.stringify(value.value);
  }
}

function formatBytes(size) {
  if (size < 1024) {
    return `${size} B`;
  }
  if (size < 1024 ** 2) {
    return `${(size / 1024).toFixed(1)} KB`;
  }
  return `${(size / 1024 ** 2).toFixed(2)} MB`;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}
