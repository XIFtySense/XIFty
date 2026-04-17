import init, {
  extract_bytes,
  probe_bytes,
} from "./pkg/xifty_wasm.js";

const fileInput = document.getElementById("file-input");
const dropZone = document.getElementById("drop-zone");
const summaryPanel = document.getElementById("summary-panel");
const resultsPanel = document.getElementById("results-panel");
const errorPanel = document.getElementById("error-panel");
const resultOutput = document.getElementById("result-output");
const statusText = document.getElementById("status-text");
const errorText = document.getElementById("error-text");
const copyJsonButton = document.getElementById("copy-json");
const tabs = Array.from(document.querySelectorAll(".tab"));

const fileNameEl = document.getElementById("file-name");
const fileSizeEl = document.getElementById("file-size");
const fileFormatEl = document.getElementById("file-format");
const fileContainerEl = document.getElementById("file-container");
const fileIssuesEl = document.getElementById("file-issues");

let currentFile = null;
let currentViews = {};
let currentView = "normalized";

await init();

fileInput.addEventListener("change", async (event) => {
  const [file] = event.target.files ?? [];
  if (!file) {
    return;
  }
  await handleFile(file);
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
  if (!currentViews[currentView]) {
    return;
  }
  await navigator.clipboard.writeText(
    JSON.stringify(currentViews[currentView], null, 2),
  );
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
  fileIssuesEl.textContent = String(probe.report?.issues?.length ?? 0);
}

function renderCurrentView() {
  const payload = currentViews[currentView];
  if (!payload) {
    resultOutput.textContent = "";
    return;
  }

  resultOutput.textContent = JSON.stringify(payload, null, 2);
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
  errorText.textContent =
    error instanceof Error ? error.message : String(error);
  statusText.textContent = "Extraction failed";
}

function hideError() {
  errorPanel.hidden = true;
  errorText.textContent = "";
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
