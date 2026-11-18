# Clothing Merge Plan

## Goal

Add clothing composition alongside map composition, sharing one raw vanilla
archive and the generic resource-store mechanisms, with each feature keeping its
own derived caches, sources and merged output.

## Decisions

- One raw vanilla archive (`asset/vanilla`) holds both content sets: `map`
  (YMAP, YBN) and `clothing` (YMT). One RPF traversal extracts whatever the run
  selects.
- `--generate-vanilla --content {all,map,clothing}` selects the content to
  extract; the default is `all`. The `--clothing --generate-vanilla` operation is
  removed in favour of it.
- An archive never loses content by regeneration: the extracted content is the
  union of the requested content and the content the existing archive records.
  Dropping content means deleting the archive.
- The GUI keeps generating `map` only.
- Pipeline-driven extraction of a missing archive extracts the content of its
  feature; an archive lacking the feature's content is an error naming the
  `--content` value to regenerate with.
- Feature artifacts move under feature directories; `--asset` still names the
  root, and every default path below it is per feature:

  ```text
  asset/
  ├── config.toml
  ├── log/
  ├── vanilla/          shared raw archive
  ├── map/              vanilla-cache, source, source-cache, overwrite,
  │                     overwrite-cache, merged, read-cache
  └── clothing/         vanilla-cache, source, source-cache, merged, ...
  ```

  A clothing operation's defaults never resolve into `asset/map`, and the reverse.
- Each feature store records its feature in `cache_info.json`; opening another
  feature's store is an error, so explicit paths cannot cross features either.
- Clothing modules reuse shared `core` services and do not import `app/map`.
  Generic resource-store mechanisms move to `core` when clothing first needs them,
  not ahead of it.
- The per-feature layout is the only layout. An option keeping artifacts directly
  under the asset root (such as `--asset-flatten`) was rejected: it would double
  the path resolution and overlap rules for every operation.
- This is a breaking change released as v7, made before the FiveM resource is
  distributed, so neither migration nor old-layout detection is provided: stores
  at the old paths are ignored and regenerated at the new ones, and overwrites are
  moved by hand.

## Phases

### 1. Unified Raw Vanilla Archive

- [x] Let `FileSelection` combine content sets: each content set contributes its
  extensions and a classifier, and the selection dispatches classification by
  extension. Keep `file_kinds` per file.
- [x] Record the archive's content sets in the manifest (`contents`), bumping
  `format_version`. A manifest without the field reads as `["map"]`, so existing
  map archives stay valid.
- [x] Add `--content` to `--generate-vanilla` and the union rule above; route map
  and clothing generation through one archive builder in `core/game_archive`,
  with app adapters supplying their content sets.
- [x] Merge the hash-name tables: one `hash_names.json` built from every selected
  file and RPF name.
- [x] Give each content set its own revision (a digest over its files' changes),
  so the map vanilla cache and the clothing counts cache are invalidated only by
  their own content, not by the other set being added or re-extracted.
- [x] Make each consumer check its content set is present and name the
  `--content` value to regenerate with when it is not.
- [x] Keep the GUI on `map`; pipeline reuse/generation extracts `map`.

Phase 1 is complete. The content sets (`ArchiveContent`) and their classifiers,
including the YMT root-type classifier, live in `core/game_archive`, so one
generator in `app/vanilla` serves the CLI, the GUI and the map pipeline without
`app/map` depending on `app/clothing`. Consumers read the archive through
`load_content`, a per-content view of the manifest; a content revision leaves out
the stages' `unchanged` counts, which span every content. The clothing counts cache
keeps its `asset/vanilla-clothing-cache` output until phase 2.

**Validation:** archive tests for each content mode, the union rule, the legacy
manifest default, per-content revisions, and that adding clothing to a map
archive leaves the map vanilla cache current; existing vanilla, clothing and
pipeline tests.

### 2. Per-Feature Asset Layout

- [x] Resolve default paths per feature in the CLI: map operations under
  `{asset}/map`, clothing operations under `{asset}/clothing`; shared
  `{asset}/vanilla`, `{asset}/config.toml`, `{asset}/log`.
- [x] Rename the clothing counts cache output to `{asset}/clothing/vanilla-cache`.
- [x] Update the web sample, `docs/usage.md`, `docs/reference.md`,
  `docs/ARCHITECTURE.md` and the Readme for the new paths.

**Validation:** CLI default-path tests for every operation (no default of one
feature under the other's directory), pipeline overlap checks with the new paths.

Phase 2 is complete. Recording the feature in each store moved to phase 3: map
stores, the merge output and the clothing counts cache each prepare and publish
their output with their own copy of the same code, so the check belongs in the
shared publication step extracted there rather than in every copy.

### 3. Shared Resource Store

Extract what the clothing source cache needs from `app/map/source_cache` and
`app/map/deploy` into `core` (for example `core/resource_store`), switching map
to the extracted code in the same step:

- [ ] Resource discovery and inventory, unique resource names, deployment
  skipping.
- [ ] Moving selected files into the cache, `history/`, restoration and
  exclusions, parameterised by a per-feature file predicate.
- [ ] Fingerprint freshness and staged publication of cache metadata.
- [ ] Record the feature in each store's `cache_info.json` at publication and
  refuse to replace a directory recording another feature.
- [ ] Deployment of merged and cloned files as managed files, with the stream
  directory layout supplied by the feature.

YMAP diffs, propagation, added-YMAP handling and parent relationships stay in
`app/map`.

**Validation:** the full map source-cache, restore, exclusion, merge and deploy
suites pass unchanged against the extracted code.

### 4. Clothing Source Cache and Merge

Scope depends on the open question below; the outline assumes combining addon
clothing packs into one collection.

- [ ] Clothing source cache: inventory each resource's clothing YMTs, creature
  metadata and drawable/texture files by Ped model and collection.
- [ ] Merge: assign each source drawable and texture a slot in the merged
  collection per Ped model and component/anchor, rename the stream files
  accordingly and write the combined YMTs and metadata.
- [ ] Record the slot assignment so that rebuilds keep existing slots stable and
  outfits referring to them stay valid.
- [ ] Deploy the merged collection as one resource through the shared store.
- [ ] Decide whether clothing overwrites are needed and, if so, their format.

**Validation:** sample packs under the web sample's clothing sources merge into
one collection with stable slots across rebuilds; source removal and addition
behave as recorded.

## Open Questions

- What clothing merge composes: combining addon packs into one collection
  (assumed above), replacing vanilla drawables, or both. Which vanilla counts it
  needs (per game build) follows from this.
- Whether the clothing merge needs vanilla-relative diffs at all, or only the
  counts cache.
- Whether `--read` and the store checks extend to clothing.

## Progress Updates

After completing a phase, mark its tasks complete, record changed decisions here,
and update `docs/ARCHITECTURE.md` to describe the resulting behavior.
