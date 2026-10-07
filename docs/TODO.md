# TODO

## Pipeline Foundation

- [ ] Dispatch stream files to separate format-specific workflows by extension; initially support YMAP and YBN without forcing them through one diff implementation.
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
- [x] Record source fingerprints and relevant vanilla-cache revisions; skip the whole cache when unchanged.
- [ ] Rebuild only entries affected by changed, added, removed, or stale source inputs.
- [x] Use the derived latest-file and YMAP relationship index without loading all vanilla YMAP bodies or generating diffs.
- [x] Run a source-wide pass to include cross-resource YMAP children of changed or unmatched parent YMAPs.
- [x] Preserve conflict paths and affected-file closure for merge processing.
- [x] Retain a source-cache generation summary log.

## Merged Output

- [ ] Add `--merge` to validate/update `source-cache`, perform only required format-specific merges against `vanilla-cache/latest`, and write results to the selected `-o` path.
- [ ] Emit source omit information alongside merged stream files.
- [ ] Later, read additional diffs from `asset/overwrite` and apply them to vanilla stream files before resource merges.
- [ ] Later, define overwrite targets as `{resourceName or vanilla}/{vanilla stream file}` and validate target/baseline identity before applying changes.
- [ ] Keep overlapping resource edits deterministic through configured resource priority and report the selected source.

## Conversion and Diff Commands

- [ ] Provide `--to-xml <path>`, `--from-xml <path>`, and `--get-diff <a> <b>` commands, each accepting `-o <path>` with `.` as the default output path.
- [ ] Make these commands use the same format dispatch and prerequisite/freshness behavior where applicable, without coupling their format-specific conversion or diff implementations.
- [ ] Reconcile direct command names (`--generate-vanilla`, `--generate-vanilla-cache`, `--generate-source-cache`, `--merge`) with `--step-name` so each stage has one unambiguous selection and force behavior.

## Ver1: YBN Merge

- [ ] Support latest-vanilla YBN merge only: locate conflicting vanilla stream files, merge supported changes against the latest vanilla file, and emit results and omit information.
- [ ] Keep YBN on its own merge path; stable semantic diff generation, baseline inference, and JSON diff output are out of scope for this version.
- [ ] Report unsupported hierarchy/shape changes and define the existing conservative fallback behavior explicitly.

## Ver2: YMAP Merge

- [ ] Compare each source YMAP with available vanilla versions and infer its source version from the difference count.
- [ ] Apply source changes to the latest vanilla YMAP using the precomputed parent/child index and range-limited file loading.
- [ ] Merge overlapping resource edits according to configured resource priority; define deterministic tie and missing-priority behavior and record the selected source.
- [ ] Keep YMAP diff/merge behavior independent from the YBN implementation.

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
- [ ] Update `docs/ARCHITECTURE.md` when the implemented workflow changes; its current YBN merge description is stale.