// The page: runs commands in `worker.js` and draws the virtual workspace as a tree, marking
// what the last run added, modified and removed.

const WORKSPACE = "/work";
const MAX_LOG_LINES = 5000;
const MAX_TEXT_PREVIEW = 512 * 1024;
const HEX_PREVIEW = 1024;

const TEXT = {
  en: {
    lead: "Runs the real merger on sample resources, entirely in your browser, and shows the files it writes.",
    pipeline: "Run pipeline",
    pipelineForce: "Run pipeline again with --force",
    read: "Read every YMAP as JSON (--read --all)",
    reset: "Reset files",
    run: "Run",
    loading: "Loading…",
    files: "Files",
    changesOnly: "Changes only",
    added: "added",
    modified: "modified",
    removed: "removed",
    log: "Log",
    output: "Output",
    file: "File",
    pickFile: "Select a file in the tree.",
    footer:
      "Sample resources come from the repository's docs/sample. Nothing is uploaded; the files live in memory and vanish on reload.",
    ready: "Ready. The workspace holds two sample resources and a vanilla YMAP.",
    running: (writes) => `Running… ${writes} writes`,
    done: (ms, writes, counts) =>
      `Finished in ${ms} ms: ${writes} writes, ${counts.added} added, ${counts.modified} modified, ${counts.removed} removed.`,
    failed: (message) => `Failed: ${message}`,
    crashed: "The merger crashed; the workspace was reset. See the log.",
    loadFailed: "The merger could not be loaded. Reload the page to try again; the log has the details.",
    binary: (size) => `Binary file, ${size}. The first ${HEX_PREVIEW} bytes:`,
    truncated: (size) => `Showing the first ${formatSize(MAX_TEXT_PREVIEW)} of ${size}.`,
    removedFile: "This file was removed by the last run.",
  },
  ja: {
    lead: "サンプルのリソースに本物のマージ処理をブラウザ内だけで実行し、書き出されるファイルを表示します。",
    pipeline: "パイプラインを実行",
    pipelineForce: "--force でもう一度実行",
    read: "全 YMAP を JSON で読む (--read --all)",
    reset: "ファイルを初期状態に戻す",
    run: "実行",
    loading: "読み込み中…",
    files: "ファイル",
    changesOnly: "変更のみ",
    added: "追加",
    modified: "変更",
    removed: "削除",
    log: "ログ",
    output: "出力",
    file: "ファイル",
    pickFile: "ツリーからファイルを選んでください。",
    footer:
      "サンプルのリソースはリポジトリの docs/sample のものです。アップロードは行わず、ファイルはメモリ上にだけあり、再読み込みで消えます。",
    ready: "準備完了。ワークスペースにはサンプルのリソース 2 つと vanilla の YMAP があります。",
    running: (writes) => `実行中… ${writes} 回の書き込み`,
    done: (ms, writes, counts) =>
      `${ms} ms で完了: 書き込み ${writes} 回、追加 ${counts.added}、変更 ${counts.modified}、削除 ${counts.removed}。`,
    failed: (message) => `失敗: ${message}`,
    crashed: "マージ処理がクラッシュしたため、ワークスペースを初期状態に戻しました。ログを確認してください。",
    loadFailed: "マージ処理を読み込めませんでした。ページを再読み込みしてください。詳細はログにあります。",
    binary: (size) => `バイナリファイル (${size})。先頭 ${HEX_PREVIEW} バイト:`,
    truncated: (size) => `${size} のうち先頭 ${formatSize(MAX_TEXT_PREVIEW)} を表示しています。`,
    removedFile: "このファイルは直前の実行で削除されました。",
  },
};

const $ = (selector) => document.querySelector(selector);
const elements = {
  language: $("#language"),
  status: $("#status"),
  args: $("#args"),
  tree: $("#tree"),
  changesOnly: $("#changes-only"),
  log: $("#log"),
  output: $("#output"),
  filePath: $("#file-path"),
  fileBody: $("#file-body"),
};

let language = localStorage.getItem("language") ?? (navigator.language.startsWith("ja") ? "ja" : "en");
let worker;
let busy = true;
/** path → entry before the last run; undefined until the first run. */
let before;
/** path → entry now. */
let current = new Map();
const expanded = new Set([WORKSPACE, `${WORKSPACE}/asset`]);
let selected;
let liveWrites = 0;
let statusFrame;

function text(key, ...args) {
  const value = TEXT[language][key] ?? TEXT.en[key];
  return typeof value === "function" ? value(...args) : value;
}

function applyLanguage() {
  document.documentElement.lang = language;
  elements.language.value = language;
  for (const element of document.querySelectorAll("[data-i18n]")) element.textContent = text(element.dataset.i18n);
}

function setStatus(message, isError = false) {
  elements.status.textContent = message;
  elements.status.classList.toggle("error", isError);
}

function setBusy(value) {
  busy = value;
  for (const button of document.querySelectorAll(".presets button, #command button")) button.disabled = value;
}

function formatSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Splits a command line on spaces, honouring single and double quotes. */
function splitArgs(line) {
  const args = [...line.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)].map((match) => match[1] ?? match[2] ?? match[3]);
  return args[0] === "mlo_merger" ? args.slice(1) : args;
}

// ---------------------------------------------------------------------------------------------
// Log and output

function appendLog(message, kind = "info") {
  const line = document.createElement("span");
  line.className = kind;
  line.textContent = `${message}\n`;
  const atBottom = elements.log.scrollTop + elements.log.clientHeight >= elements.log.scrollHeight - 4;
  elements.log.append(line);
  while (elements.log.childElementCount > MAX_LOG_LINES) elements.log.firstElementChild.remove();
  if (atBottom) elements.log.scrollTop = elements.log.scrollHeight;
}

function showOutput(result) {
  if (!result.ok) {
    elements.output.textContent = result.error;
    return;
  }
  try {
    elements.output.textContent = JSON.stringify(JSON.parse(result.output), null, 2);
  } catch {
    elements.output.textContent = result.output;
  }
}

function selectTab(name) {
  for (const tab of document.querySelectorAll("[role=tab]")) tab.setAttribute("aria-selected", tab.dataset.tab === name);
  for (const body of document.querySelectorAll(".tab-body")) body.hidden = body.id !== name;
}

// ---------------------------------------------------------------------------------------------
// Tree

function flatten(node, map = new Map()) {
  const { children, ...entry } = node;
  map.set(node.path, entry);
  for (const child of children ?? []) flatten(child, map);
  return map;
}

function parentOf(path) {
  return path.slice(0, path.lastIndexOf("/")) || "/";
}

/** The status of `path` against the snapshot before the last run. */
function statusOf(path) {
  if (!before) return undefined;
  const now = current.get(path);
  const then = before.get(path);
  if (!now) return "removed";
  if (!then) return "added";
  if (now.kind !== then.kind) return "modified";
  if (now.kind === "file" && (now.size !== then.size || now.mtime !== then.mtime)) return "modified";
  if (now.kind === "symlink" && now.target !== then.target) return "modified";
  return undefined;
}

/** Every path shown: the current ones and, after a run, the removed ones. */
function shownEntries() {
  const entries = new Map(current);
  if (before) for (const [path, entry] of before) if (!entries.has(path)) entries.set(path, entry);
  const children = new Map();
  for (const path of entries.keys()) {
    if (path === WORKSPACE) continue;
    const parent = parentOf(path);
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent).push(path);
  }
  for (const list of children.values()) {
    list.sort((a, b) => {
      const dirFirst = (entries.get(b).kind === "dir") - (entries.get(a).kind === "dir");
      return dirFirst || a.localeCompare(b);
    });
  }
  const statuses = new Map();
  const changedBelow = new Set();
  for (const path of entries.keys()) {
    const status = statusOf(path);
    if (!status) continue;
    statuses.set(path, status);
    for (let parent = parentOf(path); parent.startsWith(WORKSPACE); parent = parentOf(parent)) {
      changedBelow.add(parent);
      if (parent === WORKSPACE) break;
    }
  }
  return { entries, children, statuses, changedBelow };
}

function countChanges() {
  const counts = { added: 0, modified: 0, removed: 0 };
  const paths = new Set([...current.keys(), ...(before?.keys() ?? [])]);
  for (const path of paths) {
    const kind = (current.get(path) ?? before.get(path)).kind;
    const status = statusOf(path);
    if (status && kind !== "dir") counts[status]++;
  }
  return counts;
}

function renderTree() {
  const { entries, children, statuses, changedBelow } = shownEntries();
  const changesOnly = elements.changesOnly.checked;
  const badge = { added: "A", modified: "M", removed: "D" };

  const build = (path) => {
    const entry = entries.get(path);
    const status = statuses.get(path);
    if (changesOnly && path !== WORKSPACE && !status && !changedBelow.has(path)) return undefined;
    const item = document.createElement("li");
    if (status) item.classList.add(status);
    const isDir = entry.kind === "dir";
    if (isDir && !expanded.has(path)) item.classList.add("collapsed");

    const row = document.createElement("div");
    row.className = "row";
    if (path === selected) row.classList.add("selected");
    row.dataset.path = path;
    const toggle = document.createElement("span");
    toggle.className = "toggle";
    toggle.textContent = isDir ? (expanded.has(path) ? "▾" : "▸") : "";
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = path === WORKSPACE ? WORKSPACE : entry.name + (isDir ? "/" : "");
    const meta = document.createElement("span");
    meta.className = "meta";
    if (status) {
      const mark = document.createElement("span");
      mark.className = "badge";
      mark.textContent = `${badge[status]} `;
      meta.append(mark);
    }
    meta.append(entry.kind === "file" ? formatSize(entry.size) : entry.kind === "symlink" ? `→ ${entry.target}` : "");
    row.append(toggle, name, meta);
    item.append(row);

    if (isDir) {
      const list = document.createElement("ul");
      for (const child of children.get(path) ?? []) {
        const built = build(child);
        if (built) list.append(built);
      }
      item.append(list);
    }
    return item;
  };

  const root = document.createElement("ul");
  const top = entries.has(WORKSPACE) ? build(WORKSPACE) : undefined;
  if (top) root.append(top);
  elements.tree.replaceChildren(root);
}

function expandChanges() {
  const { changedBelow } = shownEntries();
  for (const path of changedBelow) expanded.add(path);
}

elements.tree.addEventListener("click", (event) => {
  const row = event.target.closest(".row");
  if (!row) return;
  const path = row.dataset.path;
  const entry = current.get(path) ?? before?.get(path);
  if (entry.kind === "dir") {
    if (expanded.has(path)) expanded.delete(path);
    else expanded.add(path);
    renderTree();
    return;
  }
  selected = path;
  renderTree();
  selectTab("file");
  elements.filePath.textContent = path;
  if (!current.has(path)) {
    elements.fileBody.textContent = text("removedFile");
    return;
  }
  elements.fileBody.textContent = "…";
  worker.postMessage({ type: "file", path });
});

// ---------------------------------------------------------------------------------------------
// File preview

function hexDump(bytes) {
  const lines = [];
  for (let offset = 0; offset < bytes.length; offset += 16) {
    const chunk = bytes.subarray(offset, offset + 16);
    const hex = [...chunk].map((byte) => byte.toString(16).padStart(2, "0")).join(" ");
    const ascii = [...chunk].map((byte) => (byte >= 32 && byte < 127 ? String.fromCharCode(byte) : ".")).join("");
    lines.push(`${offset.toString(16).padStart(8, "0")}  ${hex.padEnd(47)}  ${ascii}`);
  }
  return lines.join("\n");
}

function showFile(path, bytes) {
  if (path !== selected) return;
  let body;
  try {
    const content = new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(0, MAX_TEXT_PREVIEW));
    body = content;
    if (path.endsWith(".json") && bytes.length <= MAX_TEXT_PREVIEW) {
      try {
        body = JSON.stringify(JSON.parse(content), null, 2);
      } catch {
        // Not JSON after all; show it as it is.
      }
    }
    if (bytes.length > MAX_TEXT_PREVIEW) body = `${text("truncated", formatSize(bytes.length))}\n\n${body}`;
  } catch {
    body = `${text("binary", formatSize(bytes.length))}\n\n${hexDump(bytes.subarray(0, HEX_PREVIEW))}`;
  }
  elements.fileBody.textContent = body;
}

// ---------------------------------------------------------------------------------------------
// Worker

function startWorker() {
  worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });
  worker.onmessage = ({ data }) => handlers[data.type]?.(data);
  worker.onerror = (event) => handlers.failed({ error: event.message, restart: false });
}

const handlers = {
  ready({ tree }) {
    current = flatten(tree);
    before = undefined;
    renderTree();
    setBusy(false);
    setStatus(text("ready"));
  },
  log({ level, message }) {
    appendLog(message, level.toLowerCase());
  },
  change() {
    liveWrites++;
    statusFrame ??= requestAnimationFrame(() => {
      statusFrame = undefined;
      if (busy) setStatus(text("running", liveWrites));
    });
  },
  done(result) {
    before = current;
    current = flatten(result.tree);
    expandChanges();
    renderTree();
    showOutput(result);
    setBusy(false);
    const counts = countChanges();
    if (result.ok) {
      setStatus(text("done", Math.round(result.milliseconds), result.writes, counts));
    } else {
      setStatus(text("failed", result.error), true);
      selectTab("log");
    }
    if (selected && current.has(selected)) worker.postMessage({ type: "file", path: selected });
  },
  file({ path, bytes }) {
    showFile(path, bytes);
  },
  failed({ error, restart }) {
    appendLog(error, "panic");
    selectTab("log");
    worker.terminate();
    setBusy(true);
    if (restart) {
      setStatus(text("crashed"), true);
      startWorker();
    } else {
      setStatus(text("loadFailed"), true);
    }
  },
};

function runCommand(line) {
  if (busy) return;
  const args = splitArgs(line);
  elements.args.value = args.join(" ");
  appendLog(`$ mlo_merger ${args.join(" ")}`, "command");
  liveWrites = 0;
  setBusy(true);
  setStatus(text("running", 0));
  worker.postMessage({ type: "run", args });
}

// ---------------------------------------------------------------------------------------------
// Controls

for (const button of document.querySelectorAll("[data-args]")) {
  button.addEventListener("click", () => runCommand(button.dataset.args));
}

$("#command").addEventListener("submit", (event) => {
  event.preventDefault();
  runCommand(elements.args.value);
});

$("#reset").addEventListener("click", () => {
  if (busy) return;
  setBusy(true);
  selected = undefined;
  elements.log.replaceChildren();
  elements.output.textContent = "";
  worker.postMessage({ type: "reset" });
});

elements.changesOnly.addEventListener("change", renderTree);

for (const tab of document.querySelectorAll("[role=tab]")) {
  tab.addEventListener("click", () => selectTab(tab.dataset.tab));
}

elements.language.addEventListener("change", () => {
  language = elements.language.value;
  localStorage.setItem("language", language);
  applyLanguage();
  if (!busy) setStatus(text("ready"));
});

applyLanguage();
startWorker();
