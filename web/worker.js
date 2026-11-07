// Runs mlo_merger in a worker so a long, synchronous run never freezes the page.
//
// Messages in:  { type: "reset" } | { type: "run", args } | { type: "file", path }
// Messages out: { type: "ready", tree } | { type: "log", level, message }
//               | { type: "change", op, path, from? }
//               | { type: "done", ok, output | error, tree, writes, milliseconds }
//               | { type: "file", path, bytes } | { type: "failed", error, restart }

import init, { run, set_current_dir } from "./pkg/mlo_merger_web.js";
import * as vfs from "./vfs.js";

/** Where the sample workspace lives; relative paths such as `asset/source` resolve here. */
const WORKSPACE = "/work";
/** Seeded files get times from here on, a second apart, so files a run writes stand out. */
const SEEDED_AT = Date.UTC(2026, 0, 1);

const WASM_URL = new URL("pkg/mlo_merger_web_bg.wasm", import.meta.url);
const LOAD_ATTEMPTS = 3;

async function fetchBytes(url) {
  const response = await fetch(new URL(encodeURI(url), import.meta.url));
  if (!response.ok) throw new Error(`Cannot load ${url}: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

/**
 * Downloads the whole module, retrying, instead of compiling it as it streams in: a connection
 * that drops partway, as some port forwards do with large files, then costs a retry rather than
 * the page.
 */
async function loadModule() {
  let lastError;
  for (let attempt = 1; attempt <= LOAD_ATTEMPTS; attempt++) {
    try {
      const response = await fetch(WASM_URL, { cache: attempt === 1 ? "default" : "reload" });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const bytes = await response.arrayBuffer();
      const expected = Number(response.headers.get("content-length"));
      if (expected && !response.headers.has("content-encoding") && bytes.byteLength !== expected) {
        throw new Error(`received ${bytes.byteLength} of ${expected} bytes`);
      }
      return bytes;
    } catch (error) {
      lastError = error;
    }
  }
  throw new Error(`Cannot download ${WASM_URL.pathname} (${LOAD_ATTEMPTS} attempts): ${lastError?.message ?? lastError}`);
}

/**
 * Seeds the workspace with every file `sample/files.json` lists: the sample's vanilla archive,
 * resources and configuration. Each gets its own modification time, as fingerprints skip
 * hashing a file whose size and time are unchanged.
 */
async function seed() {
  vfs.reset();
  vfs.mkdirAll(WORKSPACE);
  const response = await fetch(new URL("sample/files.json", import.meta.url));
  if (!response.ok) throw new Error(`Cannot load sample/files.json: ${response.status}`);
  const paths = await response.json();
  const files = await Promise.all(paths.map(async (path) => [path, await fetchBytes(`sample/${path}`)]));
  files.forEach(([path, bytes], index) => vfs.addFile(`${WORKSPACE}/${path}`, bytes, SEEDED_AT + index * 1000));
  set_current_dir(WORKSPACE);
}

const ready = loadModule()
  .then((bytes) => init({ module_or_path: bytes }))
  .then(seed);

vfs.onChange((change) => postMessage({ type: "change", ...change }));

function onLog(level, message) {
  postMessage({ type: "log", level, message });
}

onmessage = async ({ data }) => {
  try {
    await ready;
    if (data.type === "reset") {
      await seed();
      postMessage({ type: "ready", tree: vfs.snapshot(WORKSPACE) });
    } else if (data.type === "run") {
      let writes = 0;
      const stop = vfs.onChange(() => writes++);
      const started = performance.now();
      let result;
      try {
        result = { ok: true, output: run(data.args, onLog) };
      } catch (error) {
        if (error instanceof WebAssembly.RuntimeError) throw error; // a panic
        result = { ok: false, error: String(error?.message ?? error) };
      } finally {
        stop();
      }
      postMessage({
        type: "done",
        ...result,
        writes,
        milliseconds: performance.now() - started,
        tree: vfs.snapshot(WORKSPACE),
      });
    } else if (data.type === "file") {
      postMessage({ type: "file", path: data.path, bytes: vfs.readFile(data.path) });
    }
  } catch (error) {
    // A panic leaves the WebAssembly instance unusable; the page starts a new worker.
    postMessage({ type: "failed", error: String(error?.stack ?? error), restart: true });
  }
};

ready.then(
  () => postMessage({ type: "ready", tree: vfs.snapshot(WORKSPACE) }),
  // A worker that cannot load would fail the same way again, so it is not restarted.
  (error) => postMessage({ type: "failed", error: String(error?.message ?? error), restart: false }),
);
