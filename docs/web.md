# Browser Build

`mlo_merger-all` carries the browser build in `web/`: the same `--pipeline`, `--read` and
`--restore-source-cache` as the executables of the bundle, compiled to WebAssembly from the same
commit, reading and writing an in-memory filesystem instead of the disk.

## Contents

```text
web/
  pkg/                          wasm-bindgen output, --target web
    mlo_merger_web.js           ES module: init, run, set_current_dir
    mlo_merger_web_bg.wasm
    mlo_merger_web.d.ts
    mlo_merger_web_bg.wasm.d.ts
    snippets/**                 the copy of vfs.js the module imports; keep it beside the module
  vfs.js                        the in-memory filesystem, with helpers for the page
  vfs.d.ts
  sample/                       an original sample workspace; see sample/README.md
    files.json                  every file of the sample, relative to sample/
```

The module is built with the release profile tuned for size and optimized with `wasm-opt`, and
holds no debug information; `scripts/build_web.sh --bundle DIR` builds this directory, and fails
when `wasm-opt` is missing. Under `web` in `release.json`,
`wasm_bindgen` gives the wasm-bindgen `target` and `version`, and `files` the SHA-256 of every
file in `web/`, by path relative to it. The bundle's `version` and `commit` apply to it as well.

## Loading

```js
import init, { run, set_current_dir } from "./web/pkg/mlo_merger_web.js";
import * as vfs from "./web/vfs.js";

await init({ module_or_path: new URL("./web/pkg/mlo_merger_web_bg.wasm", import.meta.url) });
vfs.addFile("/resources/my_map/stream/my_map.ymap", bytes, mtime);
set_current_dir("/resources/[tools]/my_host");
const stdout = run(["--pipeline", "--yes", "--input", "/resources"], (level, message) => {});
```

`vfs.js` and the copy under `pkg/snippets` keep one filesystem on `globalThis.__mloMergerVfs`, so
files the page adds are the files the module reads. `run` is synchronous and a pipeline can take
a while; run it in a Worker to keep a page responsive. The working directory, against which
relative paths such as the default `asset/config.toml` resolve, is `/` until
`set_current_dir` changes it. Give files distinct modification times: like the executable, the
caches skip hashing a file whose size and time are unchanged.

## `run(args, onLog)`

### Arguments

`args` is the argument vector without the executable name, one argument per element, as a
process receives it. Nothing is split, unquoted or expanded, so paths such as
`/resources/[n4u]/my resource` need no quoting. Only `--pipeline`, `--read` and
`--restore-source-cache` run; any other command throws.

### Return value

`run` returns exactly the bytes the executable writes on standard output for the same arguments
and files: one line ending in `\n`.

| Command | Standard output |
|---|---|
| `--pipeline` | the pipeline report as one line of JSON |
| `--read` | what was read as one line of JSON |
| `--restore-source-cache` | `Restored N source files.` |

Both the executable and the browser build print through `cli::command::stdout`, so the output
cannot differ; nothing else writes to standard output during these commands. Splitting the output
on `\n` and parsing the last nonempty line as JSON works for both.

On failure the executable writes nothing on standard output, prints the error message on standard
error and exits with status 1. `run` instead throws an `Error` with that message. A parse error,
and `--help`, also throw, with the text the executable would print.

### `onLog(level, message)`

The executable's standard error carries its log, without colours when it is not a terminal, as
when another process spawns it; `onLog` receives the same events while `run` is running:

| Executable's standard error | `onLog` |
|---|---|
| Log lines at `info` and above (`HH:MM:SS LEVEL message`) | one call per line: `level` is `"ERROR"`, `"WARN"` or `"INFO"`, `message` has no time, level or indentation |
| A log span's header line, such as `vanilla_cache_stage stage=0002-…`, with its events indented below | no call of its own; each event in it has a `span{fields}: ` prefix instead |
| `--read`'s progress: a bar on a terminal, otherwise `info` log lines | `"INFO"`, as when standard error is not a terminal |
| The error message printed before exiting with status 1 | `"ERROR"`, then the thrown `Error` |
| A panic message | `"PANIC"`, then a thrown `WebAssembly.RuntimeError` |

The level is fixed at `info`; `RUST_LOG` does not apply. The executable's log file
(`asset/log/`, with debug events) has no counterpart.

`onLog` must not call `run`; doing so panics. When `onLog` throws, `run` stops and throws that
same error, a panic's `WebAssembly.RuntimeError` included; the run is cut short midway, so treat
the instance as after a panic.

## Panics

A panic aborts the WebAssembly instance: `run` throws a `WebAssembly.RuntimeError`, and the
instance must not be used again, since its memory may hold state left mid-update.

`vfs.reset()` followed by `init()` does not recover. `init` keeps the instance it created on its
first call and returns it from every later call, panicked or not. To get a working instance,
load the module anew:

- start a new Worker that imports the module, as the demo page does, or
- import the module under another URL, which makes a separate module with its own instance:

  ```js
  const fresh = await import(`./web/pkg/mlo_merger_web.js?instance=${++instances}`);
  await fresh.default({ module_or_path: wasmUrl });
  fresh.set_current_dir(workingDirectory);
  ```

The new instance starts with `/` as its working directory again. The filesystem survives,
because it lives in JavaScript: files the failed run had not finished writing stay as they were
left, and files it had open stay counted as open until `vfs.reset()`. Call `vfs.reset()` and seed
the files again to start from a known state.

`scripts/test_web.mjs` checks this, and runs the three commands with resources under an absolute
root outside the working directory; `tests/host_layout.rs` runs the executable in the same layout.
