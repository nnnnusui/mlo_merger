# App Module Split Plan

## Goal

Group application workflows by product area under `app/map` and `app/clothing`,
while keeping shared game archive, file-format, configuration and platform
mechanisms outside those feature modules. Keep the CLI as a thin adapter.

The shared versioned archive builder lives under `core/game_archive`, with
separate file-selection policies for map and clothing generation. The clothing
selector records YMT kinds in stage metadata.

## Target Structure

```text
src/
├── app/
│   ├── map/
│   │   ├── pipeline.rs
│   │   ├── vanilla.rs
│   │   ├── vanilla_cache/
│   │   ├── source_cache/
│   │   ├── overwrite/
│   │   ├── overwrite_cache/
│   │   ├── merge/
│   │   └── deploy/
│   └── clothing/
│       ├── vanilla.rs
│       ├── cache.rs
│       ├── source_cache/   # when implemented
│       └── merge/          # when implemented
├── core/
│   ├── game_archive/
│   ├── format/
│   ├── config/
│   └── ...
├── cli/
├── common/
└── platform/
```

`core/game_archive` owns RPF traversal, base/update/DLC ordering, file selection
and classification, versioned stage construction, manifest handling and
publication. The feature modules provide their own selection policies and own
feature-specific parsing, caches and composition. Dependencies flow `cli -> app
-> core`; `core` must not depend on `app` or `cli`.

## Phases

### 1. Extract the Shared Game Archive Engine

- [x] Move generic RPF/DLC extraction and stage orchestration out of
  `core/vanilla` into `core/game_archive`.
- [x] Move shared selection/classification, stage, manifest and publication
  mechanisms with the engine; keep map-only hash/index behavior outside it.
- [x] Keep map and clothing entry points as adapters that pass their own file
  selection policy.
- [x] Preserve the current archive paths, stage order, `cache_info.json` shape
  apart from already-added classification attributes, and version markers.
- [x] Keep temporary re-exports only where needed to migrate callers incrementally.

Phase 1 is complete. Feature adapters provide archive metadata before the
shared publication step; manifest handling belongs to `core/game_archive`.

**Validation:** run archive, vanilla, clothing and publication tests; verify old
manifest deserialization and the YMAP/YBN/YMT output paths.

### 2. Add Application Feature Modules

- [x] Create `app/map` and `app/clothing` module roots with feature-level
  generation entry points.
- [x] Move the existing map and clothing vanilla-generation wrappers into their
  respective app modules.
- [x] Route vanilla-generation CLI and GUI dispatch through the application
  entry points; keep argument parsing and terminal concerns in `cli`.
- [x] Keep the new app entry points dependent on `core`, without a reverse
  dependency.
- [x] Route the remaining CLI workflows through app modules as their owners move
  in Phase 3.

**Validation:** run CLI parsing tests and both feature-generation test groups;
confirm command names, arguments and defaults are unchanged.

Phase 2 generation entry points are in place. Map workflows live in `app/map`,
and clothing generation and YMT classification live in `app/clothing`.

### 3. Move Feature-Owned Workflows

- [x] Move map pipeline orchestration to `app/map`.
- [x] Move map-wide store-check orchestration to `app/map`; keep the generic
  finding types and recorder in `core/check`.
- [x] Move map-specific `vanilla_cache`, `source_cache`, `overwrite`,
  `overwrite_cache`, `merge` and `deploy` modules under `app/map`.
- [x] Move the clothing counts cache under `app/clothing`.
- [x] Move raw clothing generation orchestration and YMT classification to
  `app/clothing`.
- [x] Route the remaining map CLI workflows through `app/map`, keeping parsing
  and terminal concerns in `cli`.
- [ ] Place future clothing source analysis and composition in `app/clothing`,
  reusing shared `core` services rather than importing `app/map`.
- [ ] Keep genuinely shared file formats, archive logic and low-level helpers in
  `core`, `common` or `platform` according to their ownership.

**Validation:** run the map pipeline, source-cache, overwrite, merge, deployment
and clothing tests after each owner moves. Check that core runtime modules do
not import either feature module.

Map workflow modules and CLI dispatch use `app/map`. Format-level YMAP diff
models remain in `core/format`; the cache-backed comparison context belongs to
`app/map`. Shared archive metadata and validation belong to `core/game_archive`.

### 4. Remove Transitional Paths and Close the Migration

- [x] Update imports and remove transitional `core::vanilla` and
  `core::clothing` APIs; use `core::game_archive` for shared archive mechanisms
  and `app/{map,clothing}` for feature workflows.
- [x] Accept the module-path changes as a breaking Rust API change; do not add
  compatibility facades from `core` to `app`.
- [x] Update architecture and command documentation to describe the resulting
  structure and current behavior.
- [x] Check the full Rust test suite and formatting.
- [ ] Resolve the existing strict-Clippy failure in
  `core/game_archive/build.rs::publish_cache` (`too_many_arguments`); Clippy
  passes when this pre-existing lint is allowed.

**Validation:** `cargo fmt --check` and `cargo test` pass. Strict Clippy
currently reports the existing `too_many_arguments` lint in
`core/game_archive/build.rs::publish_cache`; the migration did not change that
function. `cargo clippy --all-targets -- -D warnings -A clippy::too_many_arguments`
passes.

## Progress Updates

After completing a phase, mark its tasks complete, record any changed boundary
decisions, and update the overview tree in `docs/ARCHITECTURE.md`. Keep this plan
as the migration checklist; the architecture document should remain a concise
map of the current/target module ownership.
