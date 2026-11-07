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
/** Seeded files get this time, so files a run writes stand out. */
const SEEDED_AT = Date.UTC(2026, 0, 1);

/** `[virtual path below WORKSPACE, URL below the page]` */
const SAMPLE_FILES = [
  ["asset/config.toml", "sample/config.toml"],
  ["asset/source/[maps]/sample_resource_a/fxmanifest.lua", "sample/source/[maps]/sample_resource_a/fxmanifest.lua"],
  ["asset/source/[maps]/sample_resource_a/stream/hei_ch1_11.ymap", "sample/source/[maps]/sample_resource_a/stream/hei_ch1_11.ymap"],
  ["asset/source/sample_resource_b/fxmanifest.lua", "sample/source/sample_resource_b/fxmanifest.lua"],
  ["asset/source/sample_resource_b/stream/maps/hei_ch1_11.ymap", "sample/source/sample_resource_b/stream/maps/hei_ch1_11.ymap"],
  ["asset/overwrite/resources/sample_resource_a.json", "sample/overwrite/resources/sample_resource_a.json"],
  ["asset/overwrite/resources/sample_resource_b.json", "sample/overwrite/resources/sample_resource_b.json"],
];

const WASM_URL = new URL("pkg/mlo_merger_web_bg.wasm", import.meta.url);
const LOAD_ATTEMPTS = 3;

/** Vanilla files: `[name, URL]`, published as version `0000-base` of `asset/vanilla`. */
const VANILLA_FILES = [["hei_ch1_11.ymap", "sample/sample.ymap"]];

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

async function sha256(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * Lays out `asset/vanilla` as `--generate-vanilla` would: one version holding the files and
 * the manifest that lets the pipeline reuse it instead of reading the game.
 */
async function seedVanilla() {
  const changes = {};
  for (const [name, url] of VANILLA_FILES) {
    const bytes = await fetchBytes(url);
    const extension = name.slice(name.lastIndexOf(".") + 1);
    const object = `0000-base/${extension}/${name}`;
    vfs.addFile(`${WORKSPACE}/asset/vanilla/${object}`, bytes, SEEDED_AT);
    changes[name] = {
      previous_sha256: null,
      file: { sha256: await sha256(bytes), object, source: `base.rpf/${name}` },
    };
  }
  const manifest = {
    format_version: 1,
    game_dir: "/mnt/gtav",
    versions: [{ id: "0000-base", parent: null, archives: [], changes, unchanged: 0 }],
  };
  vfs.addFile(`${WORKSPACE}/asset/vanilla/cache_info.json`, JSON.stringify(manifest), SEEDED_AT);
}

async function seed() {
  vfs.reset();
  vfs.mkdirAll(WORKSPACE);
  const files = await Promise.all(SAMPLE_FILES.map(async ([path, url]) => [path, await fetchBytes(url)]));
  for (const [path, bytes] of files) vfs.addFile(`${WORKSPACE}/${path}`, bytes, SEEDED_AT);
  await seedVanilla();
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
