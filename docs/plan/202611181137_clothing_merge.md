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
- Clothing merge combines addon collections (a Ped model's `CPedVariationInfo`
  YMT, its shop meta and streamed drawables and textures) into one collection per
  Ped model, renumbering drawables and props. Packs shipping only drawables and
  textures join it as addon drawables. Replacements of vanilla drawables are not
  merged.
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

### 3. Addon Collection Model

Clothing merge combines addon collections into one collection per Ped model.
Before any store exists, `app/clothing` learns to read what a resource ships:

- [x] Parse addon stream file names: Ped model, collection, component or prop
  anchor, drawable index, and the rest of the name (drawable suffix and
  alternative, texture letter and race, cloth), so that a file can be renamed
  to another collection and index and nothing else. Names without a Ped model
  and collection prefix are loose files (see phase 7).
- [x] Read a collection's `CPedVariationInfo` into a model keeping every field
  needed to write it again, from RSC7 META, PSO and XML.
- [ ] Read RBF YMTs (creature metadata ships as RBF).
- [ ] Read its `ShopPedApparel` meta, creature metadata and
  `pedalternatevariations.meta` entries, each of which refers to drawables and
  props by index.
- [x] Group a resource's files into collections and report files belonging to
  none.

**Validation:** unit tests over invented names and minimal YMTs in every
container; the collections of a sample resource are recognised whole.

Progress: the `CPedVariationInfo` model is the XML tree the existing META and
PSO converters produce, with clothing field names added to the shared name
table; merging renumbers the index fields it knows and carries every other field
over. Names that do not parse are replacements of vanilla files (tattoo, overlay
and skin textures), which the merge leaves alone. Grouping reports, per resource, its collections,
the files naming a drawable or prop their YMT does not declare, loose files, and
files whose prefix names no collection of the resource (replacements of vanilla
collections, or files of a collection whose YMT is missing).

### 4. Shared Resource Store and Clothing Source Cache

The clothing source cache moves each merged resource's collection files (stream
files, YMTs, metas) into the cache, as the map source cache moves the files it
replaces, so the game loads only the merged collection. What both need moves
from `app/map/source_cache` and `app/map/deploy` into `core` (for example
`core/resource_store`), switching map to the extracted code in the same step:

- [ ] Resource discovery and inventory, unique resource names, deployment
  skipping.
- [ ] Moving selected files into the cache, `history/`, restoration and
  exclusions, parameterised by a per-feature file predicate.
- [ ] Fingerprint freshness and staged publication of cache metadata.
- [ ] Record the feature in each store's `cache_info.json` at publication and
  refuse to replace a directory recording another feature.
- [ ] Clothing source cache on top of it, inventorying collections with the
  phase 3 model.

YMAP diffs, propagation, added-YMAP handling and parent relationships stay in
`app/map`.

**Validation:** the full map source-cache, restore, exclusion, merge and deploy
suites pass unchanged against the extracted code; clothing source-cache
generation, restoration and exclusion on sample resources.

### 5. Collection Merge

- [ ] Assign each source drawable, its textures and each prop a slot in the
  merged collection per Ped model and component or anchor, in resource order.
- [ ] Record the assignment, keeping existing slots across rebuilds and
  appending new sources.
- [ ] Write the merged `CPedVariationInfo` as RSC7 META with schemas taken from
  the vanilla clothing YMTs, the shop meta, creature metadata and alternate
  variations with renumbered references.
- [ ] Rename the stream files to the merged collection and slots.
- [ ] Report, per game build, each source drawable's merged index and its index
  among all collections, using the vanilla counts cache, so saved outfits can
  be migrated.

**Validation:** sample collections merge into one with every reference
renumbered; a rebuild after adding or removing a source keeps the other slots.

### 6. Clothing Deploy and Pipeline

- [ ] Deploy the merged collection as one resource with its `fxmanifest.lua`
  `data_file` entries, through the shared deploy.
- [ ] Add a clothing pipeline: vanilla, counts cache, source cache, merge,
  deploy.
- [ ] Add clothing sources to the web sample with invented names.

**Validation:** pipeline and deploy tests; host-layout run of the clothing
pipeline.

### 7. Loose Material Packs

Packs shipping only drawables and textures (`jbib_000_u.ydd`,
`jbib_diff_000_a_uni.ytd`, …) without a Ped model prefix or YMT join the merge as
addon drawables.

- [ ] Determine their Ped model (see open questions) and component from the
  file names.
- [ ] Generate their `CPedVariationInfo` entries and shop meta entries with
  default values, and add them to the merged collection.

**Validation:** a loose pack merges with a regular collection and its drawables
appear with their textures.

## Open Questions

- How a loose pack's Ped model is decided: from directory or resource names
  (`M`/`F`, `_m`/`_f`), from a per-resource setting, or both, and what happens
  when neither tells.
- Removing a source: keep its slots as holes so later slots stay, or compact
  the collection and shift the indices behind it.
- Limits per collection (drawables per component, textures per drawable) and
  whether the merge splits into several collections when one is exceeded.
- Whether clothing overwrites are needed (leaving out drawables, ordering) and
  their format.
- Whether `--read` and the store checks extend to clothing.

## Progress Updates

After completing a phase, mark its tasks complete, record changed decisions here,
and update `docs/ARCHITECTURE.md` to describe the resulting behavior.
