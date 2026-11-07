// Runs the browser build in a bundle's `web/` directory under Node, as a host resource does:
//   node scripts/test_web.mjs path/to/web
//
// The layout matches tests/host_layout.rs: the sample's resources under a root outside the
// working directory, and the host's own `asset/` holding the configuration and vanilla archive.
// It then checks what docs/web.md says about panics: the instance is lost, `init` keeps
// returning it, and importing the module anew gives a working one.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const web = resolve(process.argv[2] ?? "web/dist");
const glue = pathToFileURL(join(web, "pkg/mlo_merger_web.js")).href;
const vfs = await import(pathToFileURL(join(web, "vfs.js")).href);
const bytes = await readFile(join(web, "pkg/mlo_merger_web_bg.wasm"));

const ROOT = "/txData/preview/resources";
const HOST = `${ROOT}/[n4u]/n4u_mlomerger`;
const MERGED = `${ROOT}/[n4u]/mlo_merged`;
const PIPELINE = [
  "--pipeline", "--yes", "--input", ROOT, "--output", MERGED,
  "--exclude", "smpl_custom_props,mlo_merged", "--gamebuild", "100",
];

/** Seeds the layout, each file a second newer than the last, as fingerprints compare times. */
async function seed() {
  vfs.reset();
  const paths = JSON.parse(await readFile(join(web, "sample/files.json"), "utf8"));
  let time = Date.UTC(2026, 0, 1);
  for (const path of paths) {
    const data = await readFile(join(web, "sample", path));
    const target = path.startsWith("asset/source/")
      ? `${ROOT}/${path.slice("asset/source/".length)}`
      : `${HOST}/${path}`;
    vfs.addFile(target, data, (time += 1000));
  }
  vfs.addFile(`${HOST}/fxmanifest.lua`, "fx_version 'cerulean'\n", (time += 1000));
}

/** The one line of standard output, which ends in a newline. */
function line(stdout) {
  assert.ok(stdout.endsWith("\n"), `Output ends in a newline: ${JSON.stringify(stdout)}`);
  const body = stdout.slice(0, -1);
  assert.ok(!body.includes("\n"), `Output is one line: ${stdout}`);
  return body;
}

const exists = (path) => {
  try {
    vfs.stat(path, true);
    return true;
  } catch {
    return false;
  }
};

async function load(url) {
  const module = await import(url);
  const exports = await module.default({ module_or_path: bytes });
  return { module, exports };
}

const logs = [];
const onLog = (level, message) => logs.push([level, message]);

// The three commands a host runs.
const first = await load(glue);
const { run, set_current_dir } = first.module;
await seed();
set_current_dir(HOST);

const report = JSON.parse(line(run(PIPELINE, onLog)));
assert.ok(report.merged.added.length > 0, JSON.stringify(report));
assert.ok(exists(`${MERGED}/stream/ymap/merged/smpl_block_01.ymap`));
assert.ok(!exists(`${ROOT}/smpl_block_rework/stream/maps/smpl_block_01.ymap`));
assert.ok(logs.some(([level, message]) => level === "INFO" && message.startsWith("Pipeline complete")));
const again = JSON.parse(line(run(PIPELINE, onLog)));
assert.deepEqual(again.merged.added, []);

assert.equal(
  run(["--restore-source-cache", "--input", ROOT, "smpl_block_rework"], onLog),
  "Restored 3 source files.\n",
);
assert.ok(exists(`${ROOT}/smpl_block_rework/stream/maps/smpl_block_01.ymap`));

const read = JSON.parse(line(run(["--read", "--source", ROOT, "smpl_block_01.ymap"], onLog)));
assert.ok(read.files.some((file) => file.origin.Resource?.resource === "smpl_block_rework"));

// A failure throws the message the executable prints, after logging it at ERROR.
logs.length = 0;
assert.throws(() => run(["--read", "--source", `${ROOT}/missing`, "smpl_block_01.ymap"], onLog), Error);
assert.ok(logs.some(([level]) => level === "ERROR"));
console.log("Commands: ok");

// A panic: calling `run` from `on_log` borrows the callback twice.
let nested = false;
assert.throws(
  () =>
    run(["--read", "--source", ROOT, "smpl_block_01.ymap"], (level, message) => {
      onLog(level, message);
      if (!nested) {
        nested = true;
        run(["--read", "--source", ROOT, "smpl_block_01.ymap"], () => {});
      }
    }),
  WebAssembly.RuntimeError,
);
assert.ok(logs.some(([level]) => level === "PANIC"));
// `init` returns the instance it already has, panicked or not.
assert.equal(await first.module.default({ module_or_path: bytes }), first.exports);

// Importing the module anew instantiates it again; the working directory starts at `/` again.
const fresh = await load(`${glue}?instance=2`);
assert.notEqual(fresh.exports, first.exports);
await seed();
fresh.module.set_current_dir(HOST);
assert.ok(JSON.parse(line(fresh.module.run(PIPELINE, onLog))).merged.added.length > 0);
console.log("Panic recovery: ok");
