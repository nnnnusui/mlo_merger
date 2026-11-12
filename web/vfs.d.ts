// Types of vfs.js, the in-memory filesystem the browser build reads and writes.
//
// Paths are absolute POSIX paths. Failures throw an `Error` whose `code` is the Linux errno
// name, such as "ENOENT" or "EISDIR".

/** An error thrown by the filesystem. */
export interface VfsError extends Error {
  code: string;
}

/** One entry of {@link snapshot}. */
export interface VfsEntry {
  name: string;
  path: string;
  kind: "file" | "dir" | "symlink";
  /** Bytes of a file; 0 otherwise. */
  size: number;
  /** Modification time, milliseconds since the epoch. */
  mtime: number;
  /** A symlink's target. */
  target?: string;
  /** A directory's entries, sorted by name. */
  children?: VfsEntry[];
}

/** A change reported to {@link onChange}; `from` is the old path of a rename. */
export interface VfsChange {
  op: "create" | "modify" | "remove" | "rename";
  path: string;
  from?: string;
}

// -------------------------------------------------------------------------------------------
// Page helpers

/** Empties the filesystem and closes every open file. */
export function reset(): void;

/**
 * Writes a file, creating its parent directories. A string is written as UTF-8; `mtime` is
 * milliseconds since the epoch.
 */
export function addFile(path: string, data: Uint8Array | string, mtime?: number): void;

/** The tree below `path`, `/` by default. */
export function snapshot(path?: string): VfsEntry;

/** Calls `listener` for every change; returns a function that stops the calls. */
export function onChange(listener: (change: VfsChange) => void): () => void;

// -------------------------------------------------------------------------------------------
// Filesystem calls, which the WebAssembly module makes and pages may use too

/** A file's contents, following symlinks. */
export function readFile(path: string): Uint8Array;
/** Creates or replaces a file; its parent directory must exist. */
export function writeFile(path: string, data: Uint8Array): void;
/** `[kind (0 file, 1 directory, 2 symlink), size, mtime]`, following a final symlink when `follow`. */
export function stat(path: string, follow: boolean): Float64Array;
/** A directory's entry names, sorted. */
export function readDir(path: string): string[];
export function mkdir(path: string): void;
/** Creates a directory and its missing parents. */
export function mkdirAll(path: string): void;
export function unlink(path: string): void;
export function rmdir(path: string): void;
/** Removes a directory tree, or a symlink itself. */
export function rmAll(path: string): void;
export function rename(from: string, to: string): void;
/** Copies a file's contents and returns their length. */
export function copyFile(from: string, to: string): number;
export function hardLink(from: string, to: string): void;
/** The path with symlinks, `.` and `..` resolved. */
export function realpath(path: string): string;
/** Sets a file's modification time, milliseconds since the epoch. */
export function setModified(path: string, milliseconds: number): void;

/** Opens a file with `flags` (1 read, 2 write, 4 append, 8 truncate, 16 create, 32 create new). */
export function open(path: string, flags: number): number;
export function fdRead(fd: number, position: number, length: number): Uint8Array;
export function fdWrite(fd: number, position: number, data: Uint8Array): void;
export function fdStat(fd: number): Float64Array;
export function fdSetModified(fd: number, milliseconds: number): void;
export function close(fd: number): void;
