# TODO

## Pipeline

- [ ] Apply `--gamebuild` across the default pipeline as an upper bound (currently rejected there; only `--generate-vanilla` and `--generate-vanilla-cache` accept a stage ID or name). Also accept numeric GTA build IDs and define their mapping to and ordering against archive stages.

## Vanilla Archive (`asset/vanilla`)

- [ ] Record source `.rpf` paths with their size and modification time in the archive manifest, and treat an existing archive as stale when they change. Currently an existing archive is reused unless it is explicitly regenerated.

## Overwrites

Per-resource overwrites (`overwrite/resources/{resourceName}.json`) currently only leave selected differences of a source YMAP's diff out of the merge.

- [ ] Support overwrites that add or change data, not only skip source differences.
- [ ] Read vanilla overwrites from `overwrite/vanilla/{version}/{fileType}/*` and apply them to that vanilla version's stream files before resource merges.
- [ ] Support YBN overwrites.
- [ ] Report missing, stale, unsupported or ambiguous overwrite changes as errors or diagnostics, rather than only warning and skipping them.

## Architecture

- [ ] Give each data owner its own read and freshness-check APIs (`core::vanilla`, `core::vanilla_cache`, `core::source_cache`, …), so merge and other consumers stop walking those directories or reading their files directly. For example, merge still enumerates `vanilla-cache/latest` itself.
- [ ] Provide the interface for applying overwrites in `core::overwrite`, and have merge read the overwrite data it needs only through it.

## Merge Policy

- [ ] Define the resource-priority configuration format, resource matching rules, and behavior for equal or unspecified priorities. Currently the first source that modifies or replaces an item wins, in source order.
- [ ] Merge overlapping resource edits by configured priority and record the selected source. Precedence must never depend on filesystem enumeration order.
- [ ] Report unsupported hierarchy/shape changes and conflicts for each format, and document the conservative fallback behavior.

## YMAP Baseline Inference

- [ ] Compare each source YMAP with the available vanilla versions and infer its source version from the number of differences.
- [ ] Apply source changes from the inferred baseline to the latest vanilla YMAP and repair parent references.

## Diff Documents

Source-cache diff documents currently cover YMAPs against the latest vanilla only.

- [ ] Write source diffs against a selected or inferred vanilla version, not only the latest one.
- [ ] Write vanilla version-to-version diffs.
- [ ] Add YBN diff documents and their TypeScript types.
- [ ] Implement `--get-diff SOURCE TARGET`; it currently only prints a mock invocation.

## FiveM Preview and Editing

- [ ] Implement the separate FiveM resource that reads `--read` JSON and writes per-resource overwrites, and validate in-game selection and rendering.
