// In-memory POSIX-like filesystem behind `crate::platform::fs` in the browser build.
//
// `src/platform/fs/web.rs` imports the functions above "Page helpers" and expects their behaviour,
// including the `code` of thrown errors, to match Linux. The page uses the helpers to seed
// files before a run and to draw the tree afterwards.
//
// Nodes are shared by every directory entry that links to them, so hard links and open file
// descriptors follow a file through renames as on disk.
//
// wasm-bindgen bundles a copy of this module under `pkg/snippets`, so the state lives on
// `globalThis`: the page's import and the copy share one filesystem.

const READ = 1;
const WRITE = 2;
const APPEND = 4;
const TRUNCATE = 8;
const CREATE = 16;
const CREATE_NEW = 32;

const KIND = { file: 0, dir: 1, symlink: 2 };
const MAX_SYMLINK_HOPS = 40;

const state = (globalThis.__mloMergerVfs ??= {
  root: undefined,
  descriptors: new Map(),
  nextFd: 3,
  listeners: new Set(),
  lastTime: 0,
});

/** Strictly increasing milliseconds, so later writes always look newer. */
function now() {
  state.lastTime = Math.max(Date.now(), state.lastTime + 0.001);
  return state.lastTime;
}

function newDir() {
  return { kind: "dir", entries: new Map(), mtime: now() };
}

function newFile() {
  return { kind: "file", data: new Uint8Array(0), size: 0, mtime: now() };
}

state.root ??= newDir();

function emit(op, path, extra = {}) {
  for (const listener of state.listeners) listener({ op, path, ...extra });
}

function fail(code, path, detail = "") {
  const error = new Error(`${code}: ${detail || code}, ${path}`);
  error.code = code;
  return error;
}

function joinPath(base, name) {
  return base === "/" ? `/${name}` : `${base}/${name}`;
}

/**
 * Resolves `path` to its parent directory, final name and node, which is undefined when the
 * entry is missing. Symlinks are followed in every component, and in the last one only when
 * `follow` is set. `path` is the real path with symlinks and `..` resolved.
 */
function locate(path, follow, hops = { count: 0 }) {
  if (typeof path !== "string" || !path.startsWith("/")) throw fail("EINVAL", path, "not absolute");
  const parts = path.split("/").filter((part) => part !== "" && part !== ".");
  const stack = [{ node: state.root, name: "" }];
  const realPath = () => "/" + stack.slice(1).map((entry) => entry.name).join("/");
  for (let index = 0; index < parts.length; index++) {
    const part = parts[index];
    const last = index === parts.length - 1;
    if (part === "..") {
      if (stack.length > 1) stack.pop();
      continue;
    }
    const dir = stack[stack.length - 1].node;
    const child = dir.entries.get(part);
    if (child?.kind === "symlink" && (!last || follow)) {
      if (++hops.count > MAX_SYMLINK_HOPS) throw fail("ELOOP", path);
      const target = child.target.startsWith("/") ? child.target : joinPath(realPath(), child.target);
      const rest = parts.slice(index + 1).join("/");
      return locate(rest ? `${target}/${rest}` : target, follow, hops);
    }
    if (last) return { parent: dir, name: part, node: child, path: joinPath(realPath(), part) };
    if (!child) throw fail("ENOENT", path);
    if (child.kind !== "dir") throw fail("ENOTDIR", path);
    stack.push({ node: child, name: part });
  }
  const top = stack[stack.length - 1];
  const parent = stack.length > 1 ? stack[stack.length - 2].node : undefined;
  return { parent, name: top.name, node: top.node, path: realPath() };
}

function existing(path, follow) {
  const location = locate(path, follow);
  if (!location.node) throw fail("ENOENT", path);
  return location;
}

function statOf(node) {
  return new Float64Array([KIND[node.kind], node.kind === "file" ? node.size : 0, node.mtime]);
}

/** Adds `node` to the directory, which records the change in its modification time. */
function link(parent, name, node) {
  parent.entries.set(name, node);
  parent.mtime = now();
}

function unlinkEntry(parent, name) {
  parent.entries.delete(name);
  parent.mtime = now();
}

function setContents(node, data) {
  node.data = new Uint8Array(data); // copies out of WebAssembly memory
  node.size = node.data.length;
  node.mtime = now();
}

function contains(ancestor, node) {
  if (ancestor === node) return true;
  if (ancestor.kind !== "dir") return false;
  for (const child of ancestor.entries.values()) if (contains(child, node)) return true;
  return false;
}

export function readFile(path) {
  const { node } = existing(path, true);
  if (node.kind === "dir") throw fail("EISDIR", path);
  return node.data.slice(0, node.size);
}

export function writeFile(path, data) {
  const location = locate(path, true);
  if (location.node?.kind === "dir") throw fail("EISDIR", path);
  if (!location.parent) throw fail("EISDIR", path);
  const node = location.node ?? newFile();
  setContents(node, data);
  if (!location.node) link(location.parent, location.name, node);
  emit(location.node ? "modify" : "create", location.path);
}

export function stat(path, follow) {
  return statOf(existing(path, follow).node);
}

export function readDir(path) {
  const { node } = existing(path, true);
  if (node.kind !== "dir") throw fail("ENOTDIR", path);
  return [...node.entries.keys()].sort();
}

export function mkdir(path) {
  const location = locate(path, false);
  if (location.node || !location.parent) throw fail("EEXIST", path);
  link(location.parent, location.name, newDir());
  emit("create", location.path);
}

export function mkdirAll(path) {
  const parts = path.split("/").filter((part) => part !== "");
  let prefix = "";
  for (const part of parts) {
    prefix += `/${part}`;
    const location = locate(prefix, true);
    if (!location.node) {
      link(location.parent, location.name, newDir());
      emit("create", location.path);
    } else if (location.node.kind !== "dir") {
      throw fail(prefix === path.replace(/\/+$/, "") ? "EEXIST" : "ENOTDIR", path);
    }
  }
}

export function unlink(path) {
  const location = existing(path, false);
  if (location.node.kind === "dir") throw fail("EISDIR", path);
  unlinkEntry(location.parent, location.name);
  emit("remove", location.path);
}

export function rmdir(path) {
  const location = existing(path, false);
  if (location.node.kind !== "dir") throw fail("ENOTDIR", path);
  if (!location.parent) throw fail("EBUSY", path);
  if (location.node.entries.size > 0) throw fail("ENOTEMPTY", path);
  unlinkEntry(location.parent, location.name);
  emit("remove", location.path);
}

/** `std::fs::remove_dir_all`: removes a directory tree, or a symlink itself. */
export function rmAll(path) {
  const location = existing(path, false);
  if (location.node.kind === "file") throw fail("ENOTDIR", path);
  if (!location.parent) throw fail("EBUSY", path);
  unlinkEntry(location.parent, location.name);
  emit("remove", location.path);
}

export function rename(from, to) {
  const source = existing(from, false);
  const target = locate(to, false);
  if (!source.parent || !target.parent) throw fail("EBUSY", from);
  if (target.node === source.node) return;
  if (source.node.kind === "dir") {
    if (target.node && target.node.kind !== "dir") throw fail("ENOTDIR", to);
    if (target.node?.entries.size > 0) throw fail("ENOTEMPTY", to);
    if (contains(source.node, target.parent)) throw fail("EINVAL", to, "into itself");
  } else if (target.node?.kind === "dir") {
    throw fail("EISDIR", to);
  }
  unlinkEntry(source.parent, source.name);
  link(target.parent, target.name, source.node);
  emit("rename", target.path, { from: source.path });
}

/** `std::fs::copy`: copies contents into a new or truncated file and returns the length. */
export function copyFile(from, to) {
  const { node } = existing(from, true);
  if (node.kind !== "file") throw fail("EINVAL", from, "not a regular file");
  writeFile(to, node.data.subarray(0, node.size));
  return node.size;
}

export function hardLink(from, to) {
  const { node } = existing(from, false);
  if (node.kind === "dir") throw fail("EPERM", from);
  const target = locate(to, false);
  if (target.node) throw fail("EEXIST", to);
  link(target.parent, target.name, node);
  emit("create", target.path);
}

export function realpath(path) {
  return existing(path, true).path;
}

export function setModified(path, milliseconds) {
  existing(path, true).node.mtime = milliseconds;
}

export function open(path, flags) {
  const writable = (flags & (WRITE | APPEND)) !== 0;
  if (!(flags & READ) && !writable) throw fail("EINVAL", path, "no access mode");
  if ((flags & (TRUNCATE | CREATE | CREATE_NEW)) && !writable) throw fail("EINVAL", path, "needs write");
  let node;
  if (flags & CREATE_NEW) {
    const location = locate(path, false);
    if (location.node) throw fail("EEXIST", path);
    node = newFile();
    link(location.parent, location.name, node);
    emit("create", location.path);
  } else {
    const location = locate(path, true);
    node = location.node;
    if (!node) {
      if (!(flags & CREATE)) throw fail("ENOENT", path);
      node = newFile();
      link(location.parent, location.name, node);
      emit("create", location.path);
    } else if (node.kind === "dir" && writable) {
      throw fail("EISDIR", path);
    } else if (flags & TRUNCATE && node.size > 0) {
      setContents(node, new Uint8Array(0));
      emit("modify", location.path);
    }
  }
  const fd = state.nextFd++;
  state.descriptors.set(fd, { node, path, readable: (flags & READ) !== 0, writable });
  return fd;
}

function descriptor(fd) {
  const entry = state.descriptors.get(fd);
  if (!entry) throw fail("EBADF", `fd ${fd}`);
  return entry;
}

export function fdRead(fd, position, length) {
  const { node, path, readable } = descriptor(fd);
  if (!readable) throw fail("EBADF", path, "not open for reading");
  if (node.kind === "dir") throw fail("EISDIR", path);
  const start = Math.min(position, node.size);
  return node.data.slice(start, Math.min(node.size, start + length));
}

export function fdWrite(fd, position, data) {
  const { node, path, writable } = descriptor(fd);
  if (!writable) throw fail("EBADF", path, "not open for writing");
  const end = position + data.length;
  if (end > node.data.length) {
    const grown = new Uint8Array(Math.max(end, node.data.length * 2, 64));
    grown.set(node.data.subarray(0, node.size));
    node.data = grown;
  }
  if (position > node.size) node.data.fill(0, node.size, position);
  node.data.set(data, position);
  node.size = Math.max(node.size, end);
  node.mtime = now();
  emit("modify", path);
}

export function fdStat(fd) {
  return statOf(descriptor(fd).node);
}

export function fdSetModified(fd, milliseconds) {
  descriptor(fd).node.mtime = milliseconds;
}

export function close(fd) {
  state.descriptors.delete(fd);
}

// ---------------------------------------------------------------------------------------------
// Page helpers

/** Empties the filesystem. */
export function reset() {
  state.root = newDir();
  state.descriptors.clear();
}

/**
 * Writes a file, creating its parent directories. `data` is a `Uint8Array` or a string;
 * `mtime` is optional milliseconds since the epoch.
 */
export function addFile(path, data, mtime) {
  mkdirAll(path.slice(0, path.lastIndexOf("/")) || "/");
  writeFile(path, typeof data === "string" ? new TextEncoder().encode(data) : data);
  if (mtime !== undefined) setModified(path, mtime);
}

/**
 * The tree below `path` for drawing:
 * `{ name, path, kind, size, mtime, target?, children? }`, children sorted by name.
 */
export function snapshot(path = "/") {
  const build = (node, name, nodePath) => {
    const entry = { name, path: nodePath, kind: node.kind, size: node.size ?? 0, mtime: node.mtime };
    if (node.kind === "symlink") entry.target = node.target;
    if (node.kind === "dir") {
      entry.children = [...node.entries.keys()]
        .sort()
        .map((child) => build(node.entries.get(child), child, joinPath(nodePath, child)));
    }
    return entry;
  };
  const location = existing(path, true);
  return build(location.node, location.name, location.path);
}

/**
 * Calls `listener({ op, path, from? })` for every change, with `op` one of `create`,
 * `modify`, `remove` and `rename`. Returns a function that stops the calls.
 */
export function onChange(listener) {
  state.listeners.add(listener);
  return () => state.listeners.delete(listener);
}
