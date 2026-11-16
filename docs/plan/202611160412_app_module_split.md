# App Module Split Plan

## Goal

Group application workflows by product area under `app/map` and `app/clothing`,
while keeping shared game archive, file-format, configuration and platform
mechanisms outside those feature modules. Keep the CLI as a thin adapter.

The current foundation already has a shared versioned archive builder under
`core/vanilla`, with separate file-selection policies for map and clothing
generation. The clothing selector records YMT kinds in stage metadata.

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

Phase 1 is complete. Existing `core::vanilla` paths remain as compatibility
re-exports while consumers move; feature adapters provide archive metadata before
the shared publication step.

**Validation:** run archive, vanilla, clothing and publication tests; verify old
manifest deserialization and the YMAP/YBN/YMT output paths.

### 2. Add Application Feature Modules

- [ ] Create `app/map` and `app/clothing` module roots with feature-level
  generation entry points.
- [ ] Move the existing map and clothing vanilla-generation wrappers into their
  respective app modules.
- [ ] Change CLI dispatch to call application entry points; keep argument parsing
  and terminal concerns in `cli`.
- [ ] Enforce the dependency direction `cli -> app -> core`.

**Validation:** run CLI parsing tests and both feature-generation test groups;
confirm command names, arguments and defaults are unchanged.

### 3. Move Feature-Owned Workflows

- [ ] Move map-specific `vanilla_cache`, `source_cache`, `overwrite`,
  `overwrite_cache`, `merge`, `deploy` and `pipeline` modules under `app/map`.
- [ ] Move clothing extraction and counts under `app/clothing`.
- [ ] Place future clothing source analysis and composition in `app/clothing`,
  reusing shared `core` services rather than importing `app/map`.
- [ ] Keep genuinely shared file formats, archive logic and low-level helpers in
  `core`, `common` or `platform` according to their ownership.

**Validation:** run the map pipeline, source-cache, overwrite, merge, deployment
and clothing tests after each owner moves. Check that shared modules do not import
either feature module.

### 4. Remove Transitional Paths and Close the Migration

- [ ] Update remaining imports and remove internal compatibility re-exports.
- [ ] Decide separately whether any public Rust paths need a compatibility facade
  or a breaking API change; do not make `core` depend on `app` to preserve paths.
- [ ] Update architecture and command documentation to describe the resulting
  structure and current behavior.
- [ ] Check the full Rust test suite, formatting and Clippy.

**Validation:** `cargo fmt --check`, `cargo test`, and
`cargo clippy --all-targets -- -D warnings`.

## Progress Updates

After completing a phase, mark its tasks complete, record any changed boundary
decisions, and update the overview tree in `docs/ARCHITECTURE.md`. Keep this plan
as the migration checklist; the architecture document should remain a concise
map of the current/target module ownership.
