# TODO

## Pipeline Foundation

- [x] Dispatch stream files to separate format-specific workflows by extension; initially support YMAP and YBN without forcing them through one diff implementation.
- [x] Run the full pipeline by default without an operation flag: generate missing vanilla, update vanilla-cache/source-cache, merge and deploy; connect `-i` to source resources and `-o` to deployment.
- [x] Share one pipeline logger, reuse valid raw vanilla, validate artifact path separation before processing, and retain derived-stage freshness checks.
- [ ] Define stable internal step names matching the artifact stages: `vanilla`, `vanilla-cache`, `source-cache`, and `merge`.
- [ ] Let every command run its required prerequisite steps, skipping valid outputs; support selecting individual stages with `--step-name` and define whether multiple names are accepted.
- [ ] Use `-f` to force regeneration of the command's own stage only; prerequisite stages still use normal freshness checks unless explicitly forced.
- [ ] Accept `-y` on all pipeline commands for non-interactive execution.
- [ ] Support custom artifact paths through options such as `--vanilla`, `--vanilla-cache`, and `--source-cache`; default generated command output to `-o .`.
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

## Merged Output

- [x] Add `--merge` to validate/update `source-cache`, perform format-specific merges against latest vanilla, and write results to the selected `-o` path.
- [x] Emit source omit information alongside merged stream files in `_omit.txt`.
- [x] Group merged stream files into extension-specific directories.
- [x] Cache merge sources, vanilla baselines, dependency groups, algorithm version and output/no-output results to remerge only invalid groups; preserve unchanged files and remove stale results atomically.
- [x] Add `--deploy` to copy merged files and remaining source-cache files into `stream/{extension}/merged` and `stream/{extension}/clone`.
- [x] Cache deployment input/output mtime, size and fingerprints to copy only needed files, repair missing/changed copies and remove stale managed output safely.
- [x] Write deployment `files.txt` with original source-relative paths of active moved cache files, including merged inputs and clones.
- [ ] Later, read additional diffs from `asset/overwrite` and apply them to vanilla stream files before resource merges.
- [ ] Later, define overwrite targets as `{resourceName or vanilla}/{vanilla stream file}` and validate target/baseline identity before applying changes.
- [ ] Keep overlapping resource edits deterministic through configured resource priority and report the selected source.

## Conversion and Diff Commands

- [x] Add read-only `--find entity-guid <GUID>` with filename filtering and XML stdout; `--diff-all` includes fingerprint-validated pre-merge vanilla/source entities.
- [x] Support case-insensitive filename glob filters in entity searches and pre-merge record selection.
- [x] Add `--find entity-position X,Y,Z --round R` with inclusive 3D distance, validated coordinates/radius and existing glob/pre-merge XML support.

- [ ] Provide `--to-xml <path>`, `--from-xml <path>`, and `--get-diff <a> <b>` commands, each accepting `-o <path>` with `.` as the default output path.
- [ ] Make these commands use the same format dispatch and prerequisite/freshness behavior where applicable, without coupling their format-specific conversion or diff implementations.
- [ ] Reconcile direct command names (`--generate-vanilla`, `--generate-vanilla-cache`, `--generate-source-cache`, `--merge`) with `--step-name` so each stage has one unambiguous selection and force behavior.

## Ver1: YBN And YMAP Merge

- [x] Support latest-vanilla YBN merge: apply supported source changes directly to the latest vanilla file and emit merged files and omit information.
- [x] Support latest-vanilla YMAP merge without historical baseline inference.
- [x] Generate only YMAPs whose final supported model or repaired parent references differ from vanilla; allow no-op merges without emitting YMAPs or omit entries.
- [x] Keep YBN on its own merge path; versioned JSON diff output is out of scope for this version.
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

- [ ] Define the JSON contract consumed by a separate FiveM resource for in-game MLO preview.
- [ ] Allow the resource to emit additional typed edits, such as removing a Light, and apply them through the validated diff path.
- [ ] Define how preview edits are returned, validated, and associated with resource/file and baseline provenance.

## Merge Policy and Follow-up

- [ ] Define the resource-priority configuration format, resource matching rules, and behavior for equal or unspecified priorities.
- [ ] Define conflict and unsupported-change reporting for each format; never make precedence depend on filesystem enumeration order.
- [x] Update `docs/ARCHITECTURE.md` when the implemented workflow changes.