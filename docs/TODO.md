# TODO

## Pipeline Foundation

- [x] Generate TypeScript report/cache/configuration bindings from Serde models with Specta during release packaging.
- [x] Provide concise English/Japanese usage guides and workflow-level architecture documentation.
- [x] Split large conversion, merge and cache modules into same-named directories by responsibility; keep small unit tests beside their implementation and module/integration tests in separate files.
- [x] Dispatch stream files to separate format-specific workflows by extension; initially support YMAP and YBN without forcing them through one diff implementation.
- [x] Run the full pipeline by default without an operation flag: generate missing vanilla, update vanilla-cache/source-cache, merge and deploy; connect `-i` to source resources and `-o` to deployment.
- [x] Share one pipeline logger, reuse valid raw vanilla, validate artifact path separation before processing, and retain derived-stage freshness checks.
- [x] Delegate raw vanilla generation/reuse to GenerateVanilla, keep pipeline steps visually separated, and unify overwrite/artifact overlap validation while allowing source nesting.
- [x] Define stable internal step names matching the artifact stages: `vanilla`, `vanilla-cache`, `source-cache`, and `merge`.
- [x] Let every command run its required prerequisite steps, skipping valid outputs; support selecting individual stages with `--step-name` and define whether multiple names are accepted.
- [x] Use `-f` to force regeneration of the command's own stage only; prerequisite stages still use normal freshness checks unless explicitly forced.
- [x] Accept `-y` on all pipeline commands for non-interactive execution.
- [x] Support custom artifact paths through options such as `--vanilla`, `--vanilla-cache`, and `--source-cache`; default generated command output to `-o .`.
- [ ] Support `--gamebuild` as either a named build/version or a number. Treat it as an upper bound: ignore vanilla versions after the selected version even when their files are present; define normalization and ordering for both forms.
- [ ] Track source inputs, relevant settings, upstream cache revisions and generated outputs per stage so only affected work is rerun. Invalidate stale results when inputs are removed, outputs are missing, or relevant settings/upstream revisions change.
- [ ] Define the incremental-run manifest, log retention, and which timestamp/fingerprint is authoritative for each artifact.

## Vanilla Archive (`asset/vanilla`)

- [ ] Add `--generate-vanilla`; also run this stage automatically when `asset/vanilla` does not exist.
- [ ] Generate the current vanilla archive overlay as versioned raw stream files grouped by version and extension, replacing the current `vanilla-cache` output role.
- [ ] Record generation/read timestamps, source `.rpf` modification times and other provenance needed to detect changed game inputs; retain per-run logs.
- [x] Generate one global `hash -> text` names map containing embedded YMAP strings and entity archetype names during `--generate-vanilla`.
- [x] Collect extensionless RPF entry-name candidates during vanilla extraction so entity archetype hashes can resolve to prop names.
- [ ] On normal runs, reuse an existing `asset/vanilla`; only regenerate it when absent, stale by the defined source checks, or explicitly forced.

## Vanilla Derived Cache (`vanilla-cache`)

- [x] Add `--generate-vanilla-cache`; also build/update this cache when absent or when its `asset/vanilla` inputs have changed.
- [x] Read versioned stream files from `asset/vanilla` and build derived data needed by later steps, including latest YMAP/YBN files and the YMAP parent/child index.
- [ ] Store the latest vanilla stream file for each supported format under `latest/`, subject to `--gamebuild`'s version ceiling.
- [x] Record the raw manifest revision and timestamps of referenced `asset/vanilla` inputs so cache freshness can be checked incrementally.
- [ ] Retain processing logs for vanilla-cache generation.
- [x] Precompute the YMAP parent/child relationships for the selected latest vanilla snapshot; parse only its effective YMAP files.
- [x] Support PSO YMAPs in shared model decoding and vanilla parent/child indexing alongside RSC7 YMAPs.
- [x] Use current latest vanilla fixtures for META XML tests and a synthetic YTYP resource for shared-writer coverage.
- [ ] During YMAP merge, load only target files and the parent/child relationship closure needed for that operation, not every vanilla YMAP body.
- [ ] Leave vanilla-to-vanilla diff generation for a later version; the current diff model is not sufficiently complete.

## Source Derived Cache (`source-cache`)

- [x] Add `--generate-source-cache` to ensure `vanilla-cache` and inventory `asset/source` without calculating diffs.
- [x] Record YMAP/YBN source paths, fingerprints, latest vanilla filename/hash matches, and cross-resource conflicts grouped by resource and format.
- [x] Record source fingerprints and per-resource vanilla-cache revisions; skip unchanged resources.
- [x] Rebuild only resources affected by changed, added, removed, or stale source inputs.
- [x] Move vanilla-named stream inputs into the source cache, retaining replaced files and fingerprint/mtime records under `_old/{timestamp}`.
- [x] Store cached and archived files by stream-relative path without a `stream/` prefix, migrating recorded legacy paths on resource checks.
- [x] Place active resource files under `source-cache/resources/{resourceName}`, migrating recorded root-level cache paths while preserving modification times.
- [x] Track resource directory/manifest updates and stream presence, including resources without streams.
- [x] Support `--generate-source-cache [resourceName]` with selection-scoped `-f` updates.
- [x] Detect same-name resource moves between input groups and preserve cached files while updating source paths and resource keys.
- [x] Use the derived latest-file and YMAP relationship index without loading all vanilla YMAP bodies or generating diffs.
- [x] Run a source-wide pass to include cross-resource YMAP children of changed or unmatched parent YMAPs.
- [x] Preserve conflict paths and affected-file closure for merge processing.
- [x] Build `conflicts` from active cached files and retain the original resource inventory's conflicts under `source_conflicts`, excluding archived files from cache scans.
- [x] Retain a source-cache generation summary log.
- [x] Emit source fingerprint progress at INFO only when that resource's fingerprints differ from cached entries.
- [x] Restore recorded active source-cache files to original source paths, rejecting conflicting destinations and invalidating source indexes after success.
- [x] Accept per-execution comma-separated resource/path exclusions for source-cache generation, merge and the default pipeline, forwarding current settings to prerequisite updates without saving an exclusions file.
- [x] Remove already cached excluded resources from active JSON indexes and archive their files so merge and deployment omit them.

## Merged Output

- [x] Load YBN/YMAP matching tolerances from `asset/config.toml`, default YBN to `0.05`, and invalidate merge results when effective settings change.
- [x] Add `--merge` to validate/update `source-cache`, perform format-specific merges against latest vanilla, and write results to the selected `-o` path.
- [x] Emit source omit information alongside merged stream files in `_omit.txt`.
- [x] Group merged stream files into extension-specific directories.
- [x] Cache merge sources, vanilla baselines, dependency groups, algorithm version and output/no-output results to remerge only invalid groups; preserve unchanged files and remove stale results atomically.
- [x] Track merge algorithm versions per file type so YMAP and YBN cache invalidation remain independent.
- [x] Write YMAP entity duplicate diagnostics to merged `duplicates.json`, including original GUIDs, applied/ignored source data and resolution reasons, retaining entries across incremental reuse.
- [x] Add `--deploy` to copy merged files and remaining source-cache files into `stream/{extension}/merged` and `stream/{extension}/clone`.
- [x] Cache deployment input/output mtime, size and fingerprints to copy only needed files, repair missing/changed copies and remove stale managed output safely.
- [x] Remove empty deployment stream directories bottom-up after updates, preserving nonempty content, links and root metadata.
- [x] Write deployment `files.txt` with original source-relative paths of active moved cache files, including merged inputs and clones.
- [x] Create a default FiveM map `fxmanifest.lua` in deployment output only when absent.
- [ ] Later, read additional diffs from `asset/overwrite` and apply them to vanilla stream files before resource merges.
- [ ] Later, define overwrite targets as `{resourceName or vanilla}/{vanilla stream file}` and validate target/baseline identity before applying changes.
- [ ] Keep overlapping resource edits deterministic through configured resource priority and report the selected source.

## Conversion and Diff Commands

- [x] Add read-only `--find entity-guid <GUID>` with filename filtering and XML stdout; `--all` includes fingerprint-validated pre-merge vanilla/source entities.
- [x] Support case-insensitive filename glob filters in entity searches and pre-merge record selection.
- [x] Add `--find entity-position X,Y,Z --radius R` with inclusive 3D distance, validated coordinates/radius and existing glob/pre-merge XML support.
- [x] Add `--find ybn-position X,Y,Z --radius R --type box` with world-space collision centers, shape filtering and existing glob/pre-merge XML support.
- [x] Add read-only `--find file-from-position X,Y,Z` using YMAP entity extents and transform-aware YBN BoxMin/BoxMax bounds; `--all` also scans source, active source-cache files and latest vanilla-cache artifacts with stderr progress.
- [x] Add `--find occlude-position X,Y,Z --radius R` to return nearby YMAP boxOccluder and occludeModel entries with file provenance.
- [x] Add `--generate-rtree-json` to write RBush-compatible YMAP/YBN bounds split into merged/source/vanilla indexes with timestamps and counts.

- [x] Add `--get-diff <source.ymap|directory>` to compare YMAPs against matching latest vanilla-cache files; `-o` is a filename for one input and an output directory for multiple inputs.
- [x] Provide `--to-xml <path>` and `--from-xml <path>` commands, each accepting `-o <path>` with `.` as the default output path.
- [x] Make these commands use the same format dispatch and prerequisite/freshness behavior where applicable, without coupling their format-specific conversion or diff implementations.
- [x] Reconcile direct command names (`--generate-vanilla`, `--generate-vanilla-cache`, `--generate-source-cache`, `--merge`) with `--step-name` so each stage has one unambiguous selection and force behavior.

## Ver1: YBN And YMAP Merge

- [x] Support latest-vanilla YBN merge: apply supported source changes directly to the latest vanilla file and emit merged files and omit information.
- [x] Support latest-vanilla YMAP merge without historical baseline inference.
- [x] Merge entity deletions with deletion-wins precedence across resources and repair references to deleted parents.
- [x] Generate only YMAPs whose final supported model or repaired parent references differ from vanilla; allow no-op merges without emitting YMAPs or omit entries.
- [x] Keep YBN on its own merge path; versioned JSON diff output is out of scope for this version.
- [x] Relocate YBN composite fixup tables and remove inherited page/octant references when rebuilding native files.
- [x] Replace the YMAP model-to-native XML conversion adapter with a direct binary writer.
- [x] Split the native YMAP writer by responsibility and share checked binary storage helpers with YBN under `format/gamefile`.
- [ ] Report unsupported hierarchy/shape changes and define the existing conservative fallback behavior explicitly.

## Ver2: YMAP Merge

- [ ] Compare each source YMAP with available vanilla versions and infer its source version from the difference count.
- [ ] Apply source changes from their inferred baseline to the latest vanilla YMAP and repair parent references.
- [ ] Load only the target files and required parent/child closure during YMAP merge.
- [ ] Merge overlapping resource edits according to configured resource priority; define deterministic tie and missing-priority behavior and record the selected source.
- [x] Keep YMAP diff/merge behavior independent from the YBN implementation.

## Ver3: JSON Diff Cache

- [ ] Make vanilla stream files available by version, not only from the latest version.
- [ ] Emit versioned JSON caches for source-versus-vanilla differences and vanilla-version-to-version differences.
- [ ] Include format, resource/file identity, selected baseline/version, provenance, and schema version without duplicating raw vanilla files.

## Ver4: Additional Diff Application

- [ ] Read `overwrite/**/*.diff.json` by resource and file and apply those changes after the normal merge.
- [ ] Validate target identity and baseline/version; report missing, stale, unsupported, or ambiguous changes instead of silently applying them.

## FiveM Preview and Editing

- [x] Define a light-only JSON contract with numeric properties, resource-local LOD/distant references, influence OBB/AABBs and generated TypeScript types.
- [x] Add cached on-demand `--to-json` exports with input/output fingerprints and dependency-aware freshness.
- [x] Accept unique resource names in `--to-json`, retaining canonical grouped IDs and rejecting ambiguous names.
- [x] Retain exact grouped resource IDs in the original file-bounds R-tree, keeping light influence calculations separate in on-demand JSON exports.
- [x] Read typed resource-local light removals from `overwrite/**/*.diff.json` during merge/pipeline, preserving other resource providers.
- [x] Validate snapshot/file identity, compact related arrays together, and support edited custom YMAPs without changing source/cache bytes.
- [x] Track edits in incremental merge freshness and include replaced custom source paths in deployment omission provenance.
- [x] Cache validated overwrite preparation with mtime/size/SHA-256 checks, invalidate on diff/source/topology changes, and commit only after successful merge.
- [x] Keep overwrite configuration in the core Pipeline alongside other input paths, without a separate compatibility execution API.
- [x] Store optional per-execution exclusions in Pipeline configuration and use a single execution API; absent or empty exclusions select all resources.
- [x] Unify source-cache execution with resource selection and exclusions in BuildSourceCache configuration.
- [x] Resolve overwrite inputs from recorded source-cache resource/file membership, with stream resolution, reads, fingerprints and preparation-cache storage encapsulated in source-cache APIs.
- [ ] Implement the separate FiveM resource and validate in-game selection/rendering.
- [ ] Extend the editor contract beyond light removals; generic post-merge overwrite application remains separate.

## Merge Policy and Follow-up

- [ ] Define the resource-priority configuration format, resource matching rules, and behavior for equal or unspecified priorities.
- [ ] Define conflict and unsupported-change reporting for each format; never make precedence depend on filesystem enumeration order.
- [x] Update `docs/ARCHITECTURE.md` when the implemented workflow changes.