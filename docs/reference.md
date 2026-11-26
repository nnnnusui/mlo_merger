# Command Reference

Every operation, its files and its behavior. Run `mlo_merger --help` for the operations and
`mlo_merger --pipeline --help` (or any other operation) for its options and defaults;
convenience aliases are in [.cargo/config.toml](../.cargo/config.toml). For calling the
executable from a FiveM resource, see [usage.md](usage.md); for the design,
[ARCHITECTURE.md](ARCHITECTURE.md).

## Pipeline

```bash
mlo_merger --pipeline
mlo_merger --pipeline -i resources -o merged_mlo --game-dir /mnt/gtav
```

The pipeline extracts vanilla only when its archive is missing, updates the vanilla and
source caches, merges, then deploys. Unchanged stages are reused. `--game-dir` defaults
to `/mnt/gtav` and `--magic` to `./magic.dat`; both are used only for missing vanilla
data (see [Vanilla extraction](development.md#vanilla-extraction)). It prints one line
of JSON (`PipelineReport`, see [usage.md](usage.md#pipeline-output)) and logs everything
else on stderr.

| Path | Default | Option |
|---|---|---|
| Source resources | `asset/map/source` | `-i` |
| Raw vanilla archive | `asset/vanilla` | `--vanilla` |
| Derived vanilla cache | `asset/map/vanilla-cache` | `--vanilla-cache` |
| Source cache | `asset/map/source-cache` | `--source-cache` |
| Merged files | `asset/map/merged` | `--merged` |
| Overwrites | `asset/map/overwrite` | `--overwrite` |
| Overwrite cache | `asset/map/overwrite-cache` | `--overwrite-cache` |
| Deployment | `asset/map/merged_mlo` | `-o` |

Every command takes `--asset DIR`, which puts each default under `DIR` instead of `asset/`,
including the config file (`--config`) and the log directory (`--log-dir`). Use
`--no-log-file` to disable the run's log file. A path given by its own option is used as
given: `--asset other --source-cache cache` reads
`other/map/vanilla-cache` and `cache`.

Below the asset directory, the vanilla archive, the config file and the log directory are
shared; every other default sits in the directory of its workflow, so a command of one
workflow never defaults into the other's:

```text
asset/
├── config.toml
├── log/
├── vanilla/      raw vanilla archive
├── map/          map stores, sources, overwrites and deployment
└── clothing/     clothing stores
```

Overlapping artifact directories are rejected. The source root can contain them: for
example, use `resources` as source while running the tool from `resources/mlo_merged`.
Resources holding `deploy_cache_info.json` are deployments, and source discovery skips them.

Caches record paths relative to their stores, so moving `asset/`, or naming the same
directories by other paths, keeps every stage current. A source root holding none of the
source cache's resources is rejected, since it is not the tree the cache was built from.

`-f` forces the derived caches, merge and deployment, but reuses an existing raw vanilla
archive. `--gamebuild` selects the game build the vanilla cache, and so merge, targets
(see [Game Builds](#game-builds)).

## Individual Operations

```bash
mlo_merger --generate-vanilla -i /mnt/gtav -o asset/vanilla --magic magic.dat
mlo_merger --generate-vanilla-cache --vanilla asset/vanilla -o asset/map/vanilla-cache
mlo_merger --generate-source-cache -i asset/map/source --vanilla-cache asset/map/vanilla-cache -o asset/map/source-cache
mlo_merger --generate-source-cache --exclude 'resourceName,[maps]/shops'
mlo_merger --generate-overwrite-cache -i asset/map/source --overwrite asset/map/overwrite -o asset/map/overwrite-cache
mlo_merger --merge --vanilla-cache asset/map/vanilla-cache --source-cache asset/map/source-cache -o asset/map/merged
mlo_merger --deploy -i asset/map/merged --source-cache asset/map/source-cache -o asset/map/merged_mlo

mlo_merger --restore-source-cache -i asset/map/source --source-cache asset/map/source-cache
mlo_merger --restore-source-cache resourceName '[maps]/resource_b'
```

Each operation brings its prerequisites up to date first; `-f` applies only to the
operation itself.

## Vanilla Generation

`--generate-vanilla` extracts raw files from the installed game's base,
title-update and DLC archives into a versioned archive. `--content` selects what is
extracted, in one pass over the archives:

- `map`: YMAP and YBN files, read by the map workflow.
- `clothing`: clothing YMTs, read by the [clothing workflow](#vanilla-clothing).
- `all` (default): both.

Explicit generation replaces the selected output directory and extracts the selected
content together with the content the existing archive already holds, so
regenerating one content keeps the other. `cache_info.json` lists the archive's
content in `contents`. The pipeline extracts `map` when the archive is missing; the
GUI always extracts `map`. An operation that reads content the archive lacks fails,
naming the `--content` value to regenerate with.

```bash
mlo_merger --generate-vanilla -i /mnt/gtav -o asset/vanilla --magic magic.dat
mlo_merger --generate-vanilla -i /mnt/gtav --magic magic.dat --content map
```

The vanilla archive retains every version in `cache_info.json`. A stage with no
additions or replacements has no directory, and extension subdirectories are
created only for files written at that stage. An empty `latest_version-{version_id}`
file at the root identifies the final listed stage, including one with no file
changes. Regeneration replaces the marker.

## Source Cache

**Source-cache generation moves vanilla-named YMAP/YBN files and every other YMAP out of
resource stream directories.** They remain available in the source cache; other files stay
in the original resources. Resource names must be unique across groups. Files are cached at
`resources/{resourceName}/{stream-relative-path}`, without the leading `stream/` or
`streams/` component. A cached file a resource replaces with a new version is kept by its
content as `history/{resourceName}/{stream-relative-path}/{sha256}.{extension}`, with its diff
document then, if any, as `{sha256}.diff.json`, so overwrites written against it can be
followed to the new version (see [Overwrites](#overwrites)); nothing removes history.

A stream file's name identifies it across resources and vanilla: files of one name are one
stream file shipped several times. The exception is a name every resource ships for itself,
`_manifest.ymf`: it is no conflict, replaces no vanilla file and is no
[added stream file](#added-stream-files), so each resource's stays in its resource.

Source-cache selection accepts a resource name or input-relative resource path; `-f`
forces that selection, not its prerequisites.

### Exclusions

Source-cache generation, merge and `--pipeline` accept `--exclude 'name,partial/path'`.
Each trimmed entry matches a case-insensitive substring of the resource path relative to
`-i`; resource names and group paths can both be used. Backslashes are normalized to `/`;
entries are literal text, not globs.

- Excluded resources stay in place, outside the inventory and the source conflict reports.
- Exclusions apply only to the current execution and are not saved. Omitting `--exclude`
  or passing `--exclude ''` means no exclusions. Pipeline and merge pass them to their
  source-cache updates.
- Previously cached excluded resources are removed from the active indexes and their
  cache directories are archived under `_old/`, keeping those files out of merge and
  clone deployment. Omitting exclusions later does not restore archived files to source.
- Legacy `source_exclusions.json` files are ignored and removed after successful
  source-cache updates.

### Restoring Resources

`--restore-source-cache` moves recorded active cache files back to their original
resource paths under `-i`, preserving modification times.

- Different existing destination contents, missing cached files and symlink paths are
  rejected before any file moves; identical destination files are kept and their cached
  duplicates removed.
- Merged and deployed output stays as it is until the next merge and deployment.
- Run it while no other cache or pipeline operation is active. The next source-cache or
  pipeline run indexes the resources again and moves the same files back into the cache.
- Given resource names or input-relative resource paths, only those resources are
  restored. Every name is checked first; an unknown or ambiguous name is an error. The
  other resources stay cached, and `--read` still finds every file, but
  `stream_conflicts.json` and `vanilla_ymaps_to_read.json` are removed, so merges fail
  until the next source-cache or pipeline run.

## Merge

Merge writes native files under `ymap/` and `ybn/`, plus `_omit.txt` for replaced source
paths, `generations/` for the changes it carries over to newer generations (see
[Newer DLC Generations](#newer-dlc-generations)) and `report/` for what it decided (see
[Merge Report](#merge-report)). It rebuilds only changed or invalid groups. Failed merges
leave the prior output intact.

### Merge Report

`report/index.json` (`MergeReportIndex`) lists each vanilla YMAP the merge has something to
report of: sources competing for one of its entities or items of another array, changes to
it carried nowhere, or its not being deployed for a successor. For each it gives its report,
what the output holds of it (`written`, `unchanged` or `omitted`), its successor, and how
many entities and items are competed for and changes carried nowhere; `by_resource` lists the reported YMAPs each resource's
changes are among.

`report/vanilla/{version}/ymap/{fileName}.json` (`YmapMergeReport`), `version` and the file
name those of the latest vanilla YMAP, holds the entities competed for, each with the change
applied and those not, with why. Removals and modifications compete for a vanilla entity.
Additions are new entities, whatever their GUID: one an earlier source makes already, an
entity with the same GUID, not 0 and given to no other entity of its source, or of the same
type, archetype and LOD level at the same position within the `ymap` matching tolerance, is
left out and reported; the others are all applied.

`items` holds the items of the other arrays competed for alike, each by its array's path in
a diff (`lod_lights`, `distant_lod_lights.items`, `box_occluders`, `car_generators`, `time_cycle_modifiers`, `instanced_data.grass_instance_list`) and the
vanilla item's `base_index`. Removals, modifications and replacements compete for a vanilla
item; an addition equal within the matching tolerances to one an earlier source makes is left
out and reported without a `base_index`. LOD light additions are all applied.

| Reason | The applied change |
|---|---|
| `identical_duplicate` | Is the same |
| `removal_wins` | Removes the entity or item |
| `newer_generation_wins` | Comes from a newer generation: the YMAP's own sources from the loaded one, carried changes from their origin |
| `earlier_source_wins` | Comes from a source merged before; for an addition, one with the same GUID or placed alike, with other values |
| `same_addition_newer` | Is the same resource's addition of the entity or item in a newer generation |

A change names its resource and source: a stream file by its `stream_path`, whose
source-cache diff document holds the change, a carried document of `generations/` with the
stage of its origin generation, or a vanilla overwrite. Its `index` is where that diff names
the entity or item: `base_index` for a removal, a modification or a replacement,
`target_index` for an addition.
The report also lists the changes to the YMAP carried nowhere (`left_behind`, see
[Newer DLC Generations](#newer-dlc-generations)).

Merge applies every source's differences from the latest vanilla file together to
vanilla. YMAP differences are read from the source cache's
`diff/{resource}/{stream-relative-path}.diff.json`, which source-cache generation keeps
while their source and vanilla files and the matching tolerances are unchanged; a missing
or stale document fails the merge, so `--merge` and the pipeline update the source cache
first. Only YMAPs that still differ from vanilla after
merging are written.

### Added Stream Files

A stream file no vanilla file is named after, one a resource adds, is diffed and merged like
the others against an empty file of its format, so everything in the source is an addition.
YMAPs are the only format whose added files are merged so far; the others are deployed as
they are.

- Of resources adding a file of the same name, every version is merged, from the resource
  whose name sorts last (the one loaded last), so its version wins conflicts, and an addition
  an earlier version makes already is left out. The merged file is always written, so it
  replaces the others. The overwrite cache's `added_stream_files.json` (`AddedStreamFiles`)
  records each added file's sources in merge order and the additions left out, each with the
  item kept in its place.
- A resource overwrite with `ignore` leaves its stream file out of the merge.
- An added file only one resource ships is written only when the merge changes it, for
  example through an overwrite or a repaired parent reference; otherwise the resource's
  file is deployed as it is.

For an added YMAP:

- The empty baseline has no name, flags, extents or block, so the source's are kept.
- An addition an earlier version makes already is an entity with the same GUID, not 0 and
  given to no other entity of its version, or of the same type, archetype and LOD level at
  the same position (within the `ymap`
  [matching tolerance](#matching-configuration)), its children then pointing at the kept
  one, or another item equal within the matching tolerances; LOD lights and distant LOD
  lights are kept. The extents cover every version's.
- The source cache's `added_ymap_relationships.json` lists the added YMAPs, as
  `{resource}/{fileName}`, under the parent each declares, as the vanilla cache's
  `ymap_relationships.json` does; related YMAPs are merged together.

### Combining Sources

- A removal wins over every other change of that vanilla item, including another source's
  replacement of it, whatever the source order. An added entity equal to a removed vanilla
  entity is dropped too, so deleting wins over re-adding.
- Otherwise the first source to modify or replace an item wins.
- Occlude models combine as triangles (see [Occlude Model Triangles](#occlude-model-triangles)):
  every source's removals apply, and every source's additions once each in source order, but
  those re-adding a removed triangle.
- Removed and modified items are named by their vanilla index. An unpaired source item
  takes the place of an unpaired vanilla item between the same kept items (a
  replacement), or is added right after the vanilla item preceding it; additions keep
  source order. Kept items stay in vanilla order, so a source's reordering of kept items
  is not reproduced.

### YMAP Fields

- `contentFlags` are OR-ed into vanilla's; the parent and IMAP link keep the first
  change. Resources' changes to name, flags, extents, block and the distant LOD light
  counts are left out, keeping vanilla's values.
- Added physics dictionaries and prop instances skip names already present; resources'
  dictionary removals are not applied, and their replacements are applied as additions.
- A [vanilla overwrite](#overwrites)'s changes to these fields apply. To keep a resource's value of one of them, write it into the vanilla
  overwrite.
- Entity extensions (light effects, doors, spawn point overrides and cloth bounds) are
  compared as a whole, ignoring number changes below the `ymap` tolerance; a change
  replaces all of the entity's extensions with the source's. A YMAP with another
  extension type fails to load.
- `[prefix_]lodlights_*.ymap` and `[prefix_]distlodlights_*.ymap` hold the `lodLights`
  and `distantLodLightsSOA` of the same lights, read by index, so a light removed from
  either is removed from both. The source cache adds the other YMAP's removals to each
  one's diff as `{"Removed": {"base_index": i, "implied": true}}`, and the document's
  `partner` records the files they were taken from. Pairs whose vanilla YMAPs differ in
  light count are left alone.
- Source-cache generation writes `implied/{resourceName}/{streamPath}.json`
  (`YmapImplied`) for a resource's YMAP of a pair that needs more to stay in step, recording
  the files of its diff document in `target` and `base`. `skip`, in the shape of an
  overwrite's, leaves out the lights it adds that the resource's other YMAP of the pair, in
  the same directory, does not add at the same index, or all of them when the resource does
  not ship it: a distant light has no direction, falloff or flags to complete a LOD light
  with, and a LOD light no position or colour. `partner`, for a resource shipping only one
  YMAP of a pair, holds the removals of the vanilla other one. Merge applies the skip before
  the resource's overwrite, and merges the removals into the other vanilla YMAP as far as
  the overwrite keeps the removals they follow from.

### Occlude Model Triangles

- Occlude models are compared as the triangles of the whole YMAP, per model `flags`, not by
  model: a diff lists, per `flags` value, the triangles `removed` and `added`, and the bounds
  follow from the triangles. A source regrouping triangles into other models, or changing
  only bounds, changes nothing.
- A triangle is its corners in winding order, starting at any of them: the game culls the
  side its normal `(b - a) x (c - a)` faces away from, so the reversed triangle is another
  one. Corners within `ymap_occlude_vertex_position` per axis are one vertex. Exact repeats
  count once.
- Merging removes each removed triangle from the model holding it, dropping models left
  empty; a removal of a triangle the YMAP lacks is skipped with a warning. It places the
  added ones so connected triangles share a model: each connected piece joins the model of
  its flags sharing most of its vertices that stays within 255 distinct vertices, and the
  others fill new models after the rest, a piece over 255 vertices split breadth-first. A
  changed model takes the exact bounds of its vertices; the others are kept as they are.

### Newer DLC Generations

DLCs ship vanilla YMAPs again under a prefix of their own, such as `lodlights_medium031`,
`hei_`, `apa_`, `vw_` and `m25_2_lodlights_medium031`, and the game loads the one each
pack's change sets leave in place. Streaming a YMAP the game does not load can load it
again beside the one it does, placing its items twice, so merges deploy only the loaded
generation and carry the changes resources make to the others over to it.

#### Which YMAPs The Game Loads

- Each DLC stage of the raw archive records its pack's device, data files and the change
  sets the game executes without a script (`dlc` in `cache_info.json`), from the title
  update's `dlc_patch/{pack}/setup2.xml` and `content.xml` where it has them, and the
  islands its island hopper files (`ISLAND_HOPPER_FILE` data files in `common/data/`)
  define, with their LOD lights YMAP. An archive built without this record loads every
  YMAP.
- The vanilla cache applies them in stage order, through the selected game build, and
  writes the result to `dlc_activity.json` (see
  [Loaded Vanilla YMAPs](usage.md#loaded-vanilla-ymaps)):
  - A base game archive is loaded unless a change set invalidates its path.
  - A pack's archive is loaded once a change set enables it, or when its data file is
    mounted enabled. One that is no data file, such as a title update patch of an archive
    the pack does not declare, is not loaded.
  - An archive a change set enables while invalidating the base game path of the same name
    stands for that path, and a later change set invalidating the path unloads it: of the
    packs replacing one archive in turn, the last one's is loaded. An archive mounted over
    a valid path (`overlay`) stands for it as well; one mounted over an invalidated path by
    a change set that does not invalidate it is not loaded.
  - An additive archive loads in addition to those standing for the path it is enabled in
    place of: enabling it unloads none of them, and no later change set unloads it.
    Additive are the archives holding an island's LOD lights, which the game loads while a
    script enables the island, such as `mpheist4`'s `island_lodlights`, and those
    `additive_archives` under `[dlc_activity]` in the config file names, spelled as
    `dlc_activity.json` keys archives.
- `generations.json` lists the stream files the game does not load in `inactive`, with why;
  YBNs load from their archives as YMAPs do.

#### Generations And Their Items

- Generations are formed from the whole raw archive, so a cache built for an older game
  build also takes in the YMAPs first shipped after it, listed in `newer_than_build`.
  YMAPs are grouped into generations by prefix and into families of the same name apart
  from the prefix and a trailing number, such as every `*distlodlights_small###`, since
  newer generations move items between those blocks. Generations are ordered by the vanilla
  stage first shipping one of their YMAPs.
- Entities are the same when their GUIDs are, wherever they are; of several entities of
  one GUID, the one within 0.01 of the other on every axis with its archetype. Other items,
  and entities whose GUID the other generation lacks, are the same when they are of one
  kind, within 0.01 of each other on every axis, and identified alike: entities by
  archetype, LOD lights by color and LOD light hash, car generators by model, box
  occluders by size and rotation, time cycle modifiers by name. Occlude model triangles are
  not paired; they are the same by their corners. An item is paired only when exactly one item of
  the other generation fits, and no other item fits that one.
- Each pair of generations of a family is stored as a unit,
  `generation-units/{key}.json`, keyed by the contents of its YMAPs, and
  `ymap_generations.json` lists the units of the selected game build. Units are kept across
  rebuilds and game builds, so a rebuild pairs only the YMAPs that changed.
- `ymap_generations.json` names, per family in `successors`, the YMAP loaded in place of each one
  that is not: of the newest newer generation pairing its items, or for a YMAP first shipped
  after the build of the newest older one, the loaded YMAP holding most of them. YMAPs not
  loaded that no loaded YMAP pairs with have no successor and are merged and deployed as
  other vanilla YMAPs are.
- Streaming a YMAP first shipped after the build loads it beside the generation the build
  loads, as streaming an older one does. The cache records each with a successor in
  `newer_files` of `cache_info.json`, with its latest version, and the source cache diffs a
  resource's YMAP of that name against it, as against any vanilla YMAP.

#### Carrying Changes

- Merge carries each source YMAP's changes, after its LOD light pair document's skip and its
  overwrite, and the removals that document implies for a vanilla YMAP the resource does
  not ship, to each item they pair with in a loaded YMAP of another generation: newer ones,
  or older ones for a YMAP first shipped after the build. YMAPs the resource ships are
  included. It writes them to
  `generations/{resourceName}/{version}/{fileName}.diff.json` (`YmapPropagatedDiff`) of
  its output, `version` naming the stage first shipping the generation the changes come
  from. Each carried removal, modification or replacement names the item it came from in
  `reason`, such as
  `"propagated from resourceName: vw_lodlights_medium031.ymap lod_lights[14]"`.
- A light removed from either YMAP of a LOD light pair is removed from both YMAPs of the
  pairs it reaches. Entities a resource adds with no parent to a YMAP with a successor are
  added to the end of the successor, with the physics dictionaries it adds there, and LOD
  lights to the end of the same YMAP of the successor pair. Of the additions a resource
  carries to a YMAP, those it adds there already, in its own YMAP or carried from a newer
  generation, are left out, matched as the versions of an added YMAP are (see
  [Added Stream Files](#added-stream-files)).
- Occlude model triangle removals are carried to each loaded YMAP of those generations
  holding the triangle, and additions to a YMAP with a successor to the successor.
- Entity parent changes and modifications of other nested arrays are not carried. When
  several changes of one generation reach one item, a removal wins and otherwise the first is
  kept. `generations/left_behind.json` (`LeftBehind`) lists the changes of YMAPs with a
  successor carried nowhere: removals of items or occlude model triangles (`array`
  `occlude_models`, with no `index`) the loaded generation lacks already (`already_absent`), other changes of such
  items (`no_loaded_counterpart`), and additions of entities with a parent or of other
  arrays (`addition_not_carried`).
- Each carried document is merged into its loaded YMAP like one more resource diff, after
  that YMAP's own sources and before its vanilla overwrite, those of newer origin
  generations first, under the same policy:

  | Change to an item of a YMAP not loaded | Change to the item it pairs with in the loaded YMAP | Result |
  |---|---|---|
  | Removed | any | Removed |
  | any | Removed | Removed |
  | Modified or replaced | none | The carried change |
  | none | Modified or replaced | The loaded YMAP's change |
  | Modified or replaced | Modified or replaced | The loaded YMAP's change |

  Between changes carried from several generations, that of the newer origin generation
  wins. Of all sources' entity additions to the loaded YMAP, those an earlier source makes
  already are left out. The [Merge Report](#merge-report) gives each change not applied
  with why.
- Merge deploys no YMAP with a successor: its sources are listed in `_omit.txt`, its record
  in `cache_info.json` and its report name the successor in `superseded_by`, and a vanilla
  overwrite of it is not applied. The successor is written when the changes carried to it
  change it, so it is deployed in place of its vanilla file.

#### YBN Generations

- YBNs form generations and families by prefix and name as YMAPs do, among the YBNs of one
  collision variant marker, such as `hi@` of `hi@hei_dt1_13_0.ybn`, in
  `ybn_generations.json`. The marker names a kind of collision: `hi@` YBNs hold detailed
  collision for weapons and cameras, and generation prefixes follow the marker. Their
  polygons match by geometry, so no units are built.
- A YBN's number indexes a spatial piece of its area, and a DLC rebuilding an area splits its
  collision again, into other pieces and files of other names. So a YBN the game does not load
  has no single successor but *candidates*, in `candidates` of `ybn_generations.json`: the
  loaded YBNs of its marker whose box overlaps its own on the ground plane, from the archives
  standing for the path its archive stands for (see [Newer DLC Generations](#newer-dlc-generations)),
  or, for one first shipped after the build, from any archive; each with its box and the share
  of the YBN's box it covers, most covering first.
- Merge carries a source YBN's changes, after its overwrite, from a YBN the game does not load
  to its candidates: a removal to every candidate holding the polygon within `ybn_position`,
  in the same collision flags group; an addition to the first candidate whose ground-plane box
  holds the polygon's, or else to the one whose box is nearest it; a child added whole to the
  most covering candidate. The changes for each candidate are written to
  `generations/{resourceName}/{version}/{fileName}/{candidate}.diff.json`
  (`YbnPropagatedDiff`). The removals no candidate holds are listed in `left_behind.json`
  with `array` `polygons` and no `index`. The carried documents are merged into each
  candidate after its own sources, those of newer origin generations first, and the source
  YBN is not deployed; the merge report names the most covering candidate as `superseded_by`.
  A YBN the game does not load with no candidate is merged and deployed as it is changed.

### Entity Parents

- `parentIndex` is compared and merged as the entity it points at, read from the source
  resource's parent YMAP or else vanilla's, and written back as an index into the final
  entity arrays. Unmodified child YMAPs whose parent's entity order changed are rebuilt
  with relinked indices.
- An entity whose parent entity was removed, or whose parent lies in a YMAP the merged
  map's `parent` no longer names, is detached with a warning: `parentIndex` becomes `-1`,
  flag 8 (parent in the parent YMAP) is cleared, and `LODTYPES_DEPTH_HD` becomes
  `LODTYPES_DEPTH_ORPHANHD`; other LOD levels are kept.
- `numChildren` is counted by the merge: changes to it in diffs and overwrites are left out,
  and each parent takes its vanilla count changed by the children the merged YMAPs add or
  remove. A vanilla YMAP whose entities' counts change is written even when no source
  replaces it.
- An index into a parent YMAP the merge cannot read is kept as it is.
- A parent without a unique, non-zero GUID is kept by index only while the array it
  indexes is unchanged; otherwise that merge group fails.

### YBN

- Collision is compared as the polygons of the whole file, grouped by the collision flags of
  the geometry child holding them: what the collision is (`MAP_WEAPON`, `MAP_DYNAMIC`, ...) and
  what collides with it (`PED`, `PROJECTILE`, ...). A polygon repeated under other flags is the
  same surface in another collision layer, so groups are kept apart.
- Polygons match by kind, material (but for its colour), corner flags, radius and positions
  within `ybn_position`, not by polygon or vertex index: triangles keep their winding from any
  corner, boxes match by their eight corners, capsules and cylinders by their endpoints either
  way round. Exact repeats within a group count once. Children holding other than these
  polygons are compared whole.
- Polygons matching vanilla that any source lacks are removed, and the new polygons of every
  source are added, those two sources add alike once. A removal wins over another source's
  material or corner-flag change of the same geometry, whatever the source order.
- Added polygons are placed so connected ones share a child: each connected piece joins the
  child of its flags sharing most of its vertices that stays within the `[ybn]` limits and 256
  materials, and the other pieces fill new children after the rest, a piece over the limits
  split breadth-first. Children left empty are dropped; changed children are rebuilt and the
  others kept as they are. The root's bounds keep vanilla's and cover every child.
- The source cache writes each source YBN's differences from the latest vanilla YBN of its name
  to a diff document, which merges read and overwrites skip within (see
  [usage](usage.md#ybn-differences)). A YBN merges when several resources replace it, an
  overwrite skips within it, changes are carried to it or its repeats are left out; one only
  one source replaces is otherwise deployed as that source has it.
- A *repeat* is a polygon the very same as an earlier one of the same flags group: positions,
  winding, corner flags and material. Repeats add work for the game's collision and nothing
  else. With `[ybn_repeats]` enabled (see [Matching Configuration](#matching-configuration)),
  every YBN the merge builds leaves them out, the first kept, and the merge also builds,
  from no resource, each loaded vanilla YBN at least `vanilla_min_share` of whose polygons
  repeat, and the YBNs only one resource replaces of those. The vanilla cache counts the
  repeats of every loaded YBN in `ybn_repeats.json` (`YbnRepeatIndex`), and
  `report/ybn_repeats.json` (`YbnRepeatsReport`) gives the repeats each merged YBN leaves out.
- Files without a vanilla baseline are skipped, or rejected when sources conflict.

## Overwrites

Overwrites are the only inputs written from outside the tool besides the sources. They are
read from `--overwrite` (default: `asset/map/overwrite`), and what they call for is derived
into the overwrite cache (`--overwrite-cache`, default `asset/map/overwrite-cache`) by
`--generate-overwrite-cache`, which `--merge` and `--pipeline` run first. The merge always
reads it, even with no overwrites, since it also records which resources each added stream file is
merged from; a missing one, or one older than the source and vanilla caches or the
overwrites, fails the merge. Overwrites apply to the stream files merges take diffs of; one of
another format is reported as `unsupported_format` and not applied.

- `resources/{resourceName}/{stream_path}.diff.json` (`StreamFileOverwrite`), one per
  stream file, lists the differences of that file's diff document to leave out of the
  merge, or sets `ignore` to leave the file out whole. The diff document is the source
  cache's, against vanilla or, for an [added stream file](#added-stream-files), an empty file of
  its format.
- `vanilla/{version}/ymap/{fileName}.diff.json` (`VanillaOverwrite`) holds differences
  written directly against the latest vanilla YMAP at `{version}/ymap/{fileName}` in the
  vanilla archive, and applies only while that version is the latest. They are merged as one more source
  after every resource's, under the same merge policy, and also into vanilla YMAPs no
  resource replaces. Unlike resources' changes, their name, flags, extents, block,
  distant LOD light count and physics dictionary changes apply (see
  [YMAP Fields](#ymap-fields)).

An overwrite leaving out differences applies only while the files it was written against are current: its
`target` and `base` fingerprints (`sha256` and `size`) must match. Otherwise it is
left out whole with a warning, and the pipeline lists it in `ignored_overwrites` on every
run until it is rewritten. A resource YMAP overwrite whose source alone changed is first
followed to the new version: the overwrite cache compares the diff document of the version it
was written against, from the source cache's history, with the current one, and rewrites the
overwrite in place when every difference it left out is still left out by the same selector
or no longer made, dropping the selectors that match nothing; otherwise it is left as it is.
Either way `{overwrite cache}/source_updates/` records it (see
[usage.md](usage.md#when-source-or-vanilla-files-change)). The overwrite cache's
`warnings.json` (`OverwriteWarnings`)
lists these; see
[usage.md](usage.md#overwrite-warnings). The document formats, selectors and what happens when files
change are described in [usage.md](usage.md#overwrite-documents); complete examples are in
[docs/sample/overwrite](sample/overwrite/README.md).

## Deploy

Deploy writes from the existing merge and source-cache data:

- Merged files: `stream/{extension}/merged/{filename}`.
- Remaining cached files: `stream/{extension}/clone/{resourceName}/{stream-relative-path}`,
  except the sources of YMAPs with a successor (`superseded_by` in the merge's `cache_info.json`).
- `files.txt`: original source-relative paths of active moved files.

Deploy skips unchanged copies, repairs missing or changed outputs, and removes stale
managed files, logging a warning for a locally modified one. Unrelated files are preserved.
Empty managed stream directories are pruned. Deploy also creates `fxmanifest.lua` with the
FiveM map declaration and `/assetpacks` dependency when absent.

## Game Builds

`--gamebuild` takes a FiveM game build number (`sv_enforceGameBuild`) or a DLC name listed
for one under `[gamebuild]` in [asset/config.toml](../asset/config.toml), such as `3095` or
`mp2023_02`. Names are spelled as
`sv_enforceGameBuild` accepts them: `xm18`, `h4` and `sum` mean `mpchristmas2018`,
`mpheist4` and `mpsum`. Stage IDs and positions are not accepted. The table follows FiveM's
sources (the builds in `code/client/launcher/GameCache.cpp` and the names in
`code/components/citizen-server-impl/include/GameBuilds.h`); add a build there when FiveM
supports a new one.

- A build number missing from the table uses the closest listed build below it, with a
  warning, so a build newer than the table targets the newest one listed.
- A build that adds no DLC pack is given by its number: FiveM's name for it, such as
  `mp2026_patch_01` for 3788, is not a stage and is not accepted.
- A build ends at the last stage before the next build's first listed DLC in
  `dlclist.xml` order, so it includes its own patch and `_g9ec` stages; a name listed
  under several builds selects the latest. A build without DLC names (such as 3323)
  includes what the build before it does, and build 1 stops at the title update.
- The build applies only to the vanilla-cache step: `--generate-vanilla` always extracts
  every stage, and the vanilla cache selects the latest files from the stages up to the build's
  last one. The base and title-update files come from the installed game, not from the
  target build.
- The pipeline, `--generate-source-cache` and `--merge` pass their `--gamebuild` to the
  vanilla-cache step they run first, so running one without it returns the cache to the
  last stage.

## Matching Configuration

Merge and diff-cache operations read [asset/config.toml](../asset/config.toml) once per
operation, or `mlo_merger.toml` next to the executable when `asset/config.toml` is missing.
Release archives ship the config as that `mlo_merger.toml`. `--config` names another file; it
applies to every command, `--gamebuild` included.

The tolerances keep rounding from passing for a change. Tools round values differently when
they save a file: CodeWalker and OpenIV, for example, write the same position with slightly
different digits. A value a resource never touched can therefore differ from vanilla by that
rounding alone, and would otherwise be detected, and merged, as a change. Differences within
the tolerances are treated as no change.

Missing files or omitted keys use these defaults:

```toml
[tolerance]
ybn_position = 0.05
ymap = 0.001
ymap_occlude_vertex_position = 0.01
ymap_box_occluder_position = 1
ymap_box_occluder_rotation = 1
ymap_car_generator_position_decimals = 3
ymap_lod_light_position_decimals = 3
ymap_grass_position = 0.01
```

| Key | Meaning |
|---|---|
| `ybn_position` | Inclusive per-axis polygon coordinate and radius matching, for every supported polygon type |
| `ymap` | Smallest change of a YMAP float field that counts: entity positions, rotations, scales and LOD distances, car generator, time cycle modifier and distant LOD light values |
| `ymap_occlude_vertex_position` | Per-axis distance, in metres, within which occlude model triangle corners are one vertex |
| `ymap_box_occluder_position` | Box occluder center tolerance, in integer stored-coordinate units (a quarter metre) |
| `ymap_box_occluder_rotation` | `iCosZ`/`iSinZ` changes up to and including this many stored units are ignored (0 compares exactly) |
| `ymap_car_generator_position_decimals` | Decimal places (0 to 6) car generator positions are rounded to when pairing |
| `ymap_lod_light_position_decimals` | The same for distant LOD light positions |
| `ymap_grass_position` | Per-axis world-space distance, in metres, within which grass instances are the same one |

- Position tolerances end in `_position`. The former names `ybn` and `ymap_box_occluder` are
  still read.
- YMAP comparisons are exclusive of the boundary; extents compare exactly.
- An entity rotation within the `ymap` tolerance of the other's negation is unchanged: a
  quaternion and its negation make the same turn.
- All other values must be positive; floating values must also be finite. Unknown keys and
  invalid TOML are errors.
- `[ybn]` sets how merged YBN geometry children are filled (see [YBN](#ybn)):

  ```toml
  [ybn]
  max_geometry_vertices = 65534
  max_geometry_polygons = 32767
  ```

  `max_geometry_vertices` is at most 65534, the vertex indices being 16-bit, and
  `max_geometry_polygons` at most 32767, the GeometryBVH indexing polygons with 16-bit signed
  indices.
- `[ybn_repeats]` sets whether merges leave repeated polygons out of YBNs (see [YBN](#ybn)),
  and which vanilla YBNs no resource changes they build for that alone, by the least share
  (0 to 1) of their polygons that repeat. Without the table, or without `enabled`, it is off;
  the shipped config file turns it on:

  ```toml
  [ybn_repeats]
  enabled = true
  vanilla_min_share = 0.05
  ```
- Effective settings are part of merge freshness, so changing them regenerates on the
  next merge without `-f`. Larger tolerances can treat intentionally different nearby
  shapes as identical.

### Entity GUIDs Rounded Through A Float

Some tools pass entity GUIDs through a 32-bit float when they save a YMAP. A float holds
integers exactly only up to 2^24, so such a file stores most GUIDs with their low bits
changed, such as `923442624` for vanilla's `923442616`, and every vanilla entity it keeps
would otherwise compare as removed and added again.

A modded entity pairs with a vanilla one by the vanilla GUID when:

- neither GUID is in the other YMAP, and no other modded entity has the modded GUID;
- both GUIDs are equal as 32-bit floats;
- both entities have the same type, archetype and LOD level, at the same position within
  the `ymap` tolerance.

The pair compares as any other, the GUID aside: the merged entity keeps the vanilla GUID.
Parents named by a rounded GUID, in the YMAP itself or in the resource's parent YMAP, are
compared and merged as the vanilla entity's.

## Inspect Existing Data

```bash
mlo_merger --find entity-guid 2443198849 --filter '*cs4_10_strm_0.ymap'
mlo_merger --find entity-position 3.5,4.2,0.0 --radius 1.0
mlo_merger --find ybn-position 1211.126,-507.5948,67.54723 --radius 1.0 --type box
mlo_merger --find file-from-position 1211.126,-507.5948,67.54723
mlo_merger --find occlude-position 3.5,4.2,0.0 --radius 1.0
```

Searches write XML to stdout.

- `entity-guid` searches by entity GUID. Position searches accept `X,Y,Z`; use `--radius`
  for `entity-position`, `ybn-position` and `occlude-position` (default: `1.0`).
  `file-from-position` finds files whose spatial bounds contain the position and does not
  accept `--radius`.
- `ybn-position` searches world-space collision shape centers, not any point inside a
  shape. `--type` accepts `box`, `triangle`, `sphere`, `capsule` or `cylinder`; omit it
  to search all supported shapes. Unsupported shapes are skipped.
- `occlude-position` measures box occluders from their center, converted from stored
  quarter-metre units, and occlude models from the nearest point of their bounds.
- `-i` selects a merged directory (default: `asset/map/merged`). `--filter` accepts a
  case-insensitive filename glob; quote wildcard patterns. Duplicate occurrences are kept.
- With `--all`, the inputs the merge recorded are read from `--source`, `--source-cache` and
  `--vanilla-cache` (defaults under `asset/`), which the merge records names relative to.
- `--all` includes recorded vanilla and source inputs from before the merge, identified
  by resource and path. A changed input fingerprint is an error requiring a fresh merge.
  This is a snapshot comparison, not a computed semantic diff.
- Position searches include the 3D radius boundary. Negative coordinates are supported;
  non-finite or out-of-range coordinates and negative or non-finite radii are rejected.
  For entities, radius `0` selects the exact native-precision position. YBN indices are
  input-local and may change on rebuild.

## Read Stream File Models

```bash
mlo_merger --read --resource resourceName apa_ch2_occl_05.ymap
mlo_merger --read --resource '[maps]/resource_b'
mlo_merger --read --vanilla apa_ch2_occl_05.ymap
mlo_merger --read --vanilla --gamebuild 2802 hei_ch1_11.ymap
mlo_merger --read apa_ch2_occl_05.ymap
mlo_merger --read apa_ch2_occl_05.ymap hei_ch1_11.ymap
mlo_merger --read --all
```

`--read` writes each YMAP's and YBN's model as minified JSON, with the path
and fingerprint of the file it was read from, the vanilla `hash_names.json` that named its
hashes, for a resource's file the source cache's `hash_names.json` that named those vanilla
leaves unnamed, and the resolved `entity_lod_parents` for a YMAP (`YmapReadDocument`); and the
bound tree with named collision and material flags for a YBN (`YbnReadDocument`); see
[usage](usage.md#--read). A resource's file is written to
`asset/map/read-cache/resources/{resourceName}/{stream-relative-path}.json` and a vanilla
file to `asset/map/read-cache/vanilla/{version}/{extension}/{fileName}.json`, where `{version}` is the
version folder holding the file in the vanilla archive: the last version, up to the stage
read, that changed it. Versions holding the same file share its document. A JSON is
written again when its file, hash names or resolved LOD parents change, or with `-f`.

Vanilla files are read from the stage the vanilla cache was built for (`latest_version` in
`asset/map/vanilla-cache/cache_info.json`, `--vanilla-cache` to change the cache), so they match
the merge and the cache's `stream_file_rtree.json`; without a vanilla cache, or one built
for a stage the archive lacks, from the last stage. `--gamebuild BUILD` reads them as of a
game build instead, resolved as for the other commands (see [Game Builds](#game-builds)).
`--gamebuild` also selects the vanilla stage used to resolve parent YMAPs.

- `--resource NAME [FILE...]` reads one resource, by name or source-relative ID, from the
  existing source cache. Without a file it reads all of the resource's stream files it supports. A file name
  matching several of the resource's files is an error; give its stream-relative path
  instead.
- `--vanilla [FILE...]` reads vanilla files only. Without a file it reads all of the
  stage's stream files it supports.
- Files alone are read from every resource holding them and from vanilla. If any resource
  holds several files of one name, nothing is written.
- `--all` reads every stream file it supports of every resource and of vanilla; it takes no
  file names.
- Several files are read together in the order given; a file named twice is read once.
  If any name matches no file, nothing is written.
- Files are read and written in parallel; the result keeps the order above.

The source cache and vanilla archive default to `asset/map/source-cache` and `asset/vanilla`
(`--source-cache`, `--vanilla-dir`), and resource files not moved into the source cache are
read from `--source` (default: `asset/map/source`); `-o` changes the output root. Each document
records the file it was read from relative to its store. The command prints a
`ReadResult` JSON line listing each input, its output and whether it was written.

## Check The Stores

```bash
mlo_merger --check
mlo_merger --check --deep --merged asset/map/merged -o asset/map/merged_mlo
```

`--check` compares every store with its own metadata, prints one
line of JSON (`CheckReport`) and fails when it finds an inconsistency. Each issue names its
`store`, its `kind` and the `path` relative to the store; stores whose directory does not
exist are skipped. Store paths take the same options and defaults as the other commands.

| `kind` | Meaning |
|---|---|
| `missing` | The metadata records the file, but it does not exist |
| `unrecorded` | The file exists, but the metadata does not record it, such as a stray file in `source-cache/resources/` or a leftover staging directory |
| `changed` | The file's size, or with `--deep` its SHA-256, differs from the record |
| `unreadable` | The store's metadata cannot be read, so the store was not checked further |
| `stale` | The overwrite cache was built from other source or vanilla caches or overwrites than the current ones |

- The vanilla archive, vanilla cache, source cache (including diff documents) and merge
  output are checked whole. In the deployment, only the managed `stream/{extension}/merged/`
  and `stream/{extension}/clone/` directories are checked for unrecorded files, since other
  files are left to the user.
- The overwrite cache is checked against what it was built from (`--overwrite`, default
  `asset/map/overwrite`), and for the files it names: the sources of `added_stream_files.json`
  in the source cache, and the earlier versions `source_updates/` records in the source
  cache's history; one that is missing is reported as `missing` on the record naming it.
- Without `--deep`, contents are compared by size; the vanilla archive and cache record no
  sizes, so only `--deep` compares their contents. `--deep` reads every file, a few seconds
  for a full vanilla archive.
- The vanilla cache's latest files are checked in the vanilla archive its metadata names,
  and reported by their path from the cache directory, such as
  `../vanilla/0000-base/ymap/{file name}`.
- Nothing is repaired. The next merge and deployment rebuild their missing or changed
  files. A damaged vanilla archive is reused as it is, so extract it again with
  `--generate-vanilla`; a source-cache file that is missing
  is restored from its resource when the resource still has it, and a stale overwrite cache
  is rebuilt by `--generate-overwrite-cache` or a command running it. Unrecorded files are left
  for you to remove.

## Convert Files

```bash
mlo_merger --to-xml collision.ybn -o exported
mlo_merger --from-xml exported/collision.ybn.xml -o rebuilt
mlo_merger --from-xml map.ymap.xml -o rebuilt --vanilla asset/vanilla
```

Inputs can be files or directories. Conversion output defaults to the current directory.
META resources need compatible schemas; `--vanilla` supplies schema or template inputs.
YBN conversion needs no schemas.

## TypeScript Types

The release archives contain `asset/gen_mlo_merger.ts`, generated from the Rust Serde
models: the merge report, vanilla/source/merge/deploy caches, stream conflicts,
diff-cache reports, source-cache YMAP diff documents (`YmapComparisonDiff`), overwrite
documents (`StreamFileOverwrite`, `VanillaOverwrite`), the overwrite cache's added stream files
and warnings (`AddedStreamFiles`, `OverwriteWarnings`), `--read` documents and results (`YmapReadDocument`,
`ReadResult`), pipeline output (`PipelineReport`), version listings and
matching configuration. It does not include YBN difference payloads or JSON
Schema documents, and it is not checked into the repository; see
[development.md](development.md#typescript-types) to generate it locally.

```typescript
import type { MatchingConfig, MergeReportIndex } from "./asset/gen_mlo_merger";

const config: MatchingConfig = { tolerance: { ybn_position: 0.05 } };
```

Types preserve JSON field names, nullability and optional inputs. Paths are strings; sets
are arrays. File sizes and timestamps are JSON numbers, represented as TypeScript
`number`; values above `Number.MAX_SAFE_INTEGER` need a precision-aware JSON reader.

## Limitations

- Merge uses the latest selected vanilla, without inferring which vanilla version a
  source was made from. That inference is deferred: for YMAPs it matters only for MLOs
  made from a vanilla version older than mp2025_02 (apart from some LOD light YMAPs,
  versions up to patchday27ng, before GameBuild 2699), and it needs a way to track
  differences exactly. The code writing vanilla YMAP version-to-version diffs is kept but
  not called; see [vanillaDiff.md](vanillaDiff.md).
- Which [DLC generations](#newer-dlc-generations) the game loads is worked out from the
  installed game's files. For an older game build FiveM mounts that build's own title update,
  with its `dlc_patch/` replacements of packs' `content.xml`, `dlclist.xml` and
  `lodlights.rpf`, while the raw archive records the installed title update's for every
  build. The engine's rules are inferred from the packs' data and observed loads, such as
  island LOD lights loading beside the archives they would otherwise replace.
- `--get-diff SOURCE` writes the source YMAP's differences from the latest vanilla cache
  in the source cache's diff form; `--get-diff SOURCE TARGET` only prints a mock
  invocation.
- Cache stages do not reproduce the complete game-engine mount rules or historical
  installations.
- MLO semantic differences are reports, not lossless history patches.
- Conversion support depends on the resource family and available schemas. PSO
  rebuilding cannot grow arrays or allocated strings; RBF YMT is unsupported.
- Matching XML or stable rebuilds does not establish in-game load safety. Validate
  generated resources in the target game environment.

## Vanilla Clothing

### Collecting Definitions

`--generate-vanilla --content clothing` (or the default `all`) collects clothing and
appearance variation YMTs for all Peds, including protagonists, NPCs and online
freemode Peds, from the base game, title updates and ordered DLCs, including subpacks
and DLC patches, into the [vanilla archive](#vanilla-generation).

```bash
mlo_merger --generate-vanilla -i /mnt/gtav --magic magic.dat --content clothing
```

YMTs are selected by their root type in RSC7 META, PSO, RBF or XML, not their
filename or directory: `CPedVariationInfo` clothing/appearance definitions and
`CCreatureMetaData` creature metadata. Both are stored in their raw format as
`{version}/ymt/{filename}`; `cache_info.json` records their `file_kinds` as
`creature_metadata` or `ped_variation`. Drawable and texture assets and other YMT
types are outside this collection. Classification errors stop extraction with the
source path in the error. Within a stage, the last processed same-name definition
wins; differing contents produce a warning.

### Derived Counts

`--clothing --generate-vanilla-cache` counts each stage's clothing definitions in the
vanilla archive into `asset/clothing/vanilla-cache` (`-i` selects the archive, `-o` the
output).

```bash
mlo_merger --clothing --generate-vanilla-cache
```

Only `ped_variation` YMTs are counted. Each changed file is counted whole:
drawables and textures per component slot (`HEAD`, `UPPR`, …, named through
`availComp`), and props per anchor (`ANCHOR_HEAD`, …). Freemode Peds get one YMT
per DLC, so a stage's counts are that DLC's clothing; a replaced file's counts
are cumulative at that stage.
`creature_metadata` YMTs remain in the archive but are excluded from clothing counts.

- `summary.json`: one row per stage with file and count totals, including stages
  without changes.
- `counts/{version}.json`: per-file component and prop counts with file and stage
  totals, only for stages with changed files.
- `cache_info.json`: the digest of the archive's clothing content the counts were
  taken from. A matching cache is reused; `-f` and changes to the clothing content
  rebuild it.
