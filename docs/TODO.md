# TODO

## Vanilla Archive (`asset/vanilla`)

- [ ] Record source `.rpf` paths with their size and modification time in the archive manifest, and treat an existing archive as stale when they change. Currently an existing archive is reused unless it is explicitly regenerated.

## Overwrites

Resource overwrites (`overwrite/resources/{resourceName}/{stream_path}.diff.json`) leave selected differences of a source YMAP's diff out of the merge, or the file whole. The overwrite cache derives which stream files each YMAP that resources add is merged from, and the warnings. Vanilla overwrites (`overwrite/vanilla/latest/ymap/*.diff.json`) add differences from the latest vanilla YMAP as the last merge source.

- [ ] Let a vanilla overwrite take precedence over resources' changes to the same items, if the merge policy gains configured priorities.
- [ ] Support YBN overwrites.
- [ ] Report selectors that match no difference, unsupported or ambiguous overwrite changes, and overwrite files that are not read (such as the earlier `resources/{resourceName}.json` layout), in the overwrite cache's `warnings.json`, as `ignored_overwrites` does for missing and stale overwrites; they are only logged now.

## Architecture

- [ ] Give each data owner its own read and freshness-check APIs (`core::vanilla`, `core::vanilla_cache`, `core::source_cache`, …), so merge and other consumers stop walking those directories or reading their files directly. For example, merge still enumerates `vanilla-cache/latest` itself.

## Merge Policy

- [ ] Define the resource-priority configuration format, resource matching rules, and behavior for equal or unspecified priorities. Currently the first source that modifies or replaces an item wins, in source order.
- [ ] Merge overlapping resource edits by configured priority and record the selected source. Precedence must never depend on filesystem enumeration order.
- [ ] Report unsupported hierarchy/shape changes and conflicts for each format, and document the conservative fallback behavior.

## YMAP Baseline Inference

- [-] Compare each source YMAP with the available vanilla versions and infer its source version from the number of differences.
- [-] Apply source changes from the inferred baseline to the latest vanilla YMAP and repair parent references.

## Diff Documents

Source-cache diff documents currently cover YMAPs against the latest vanilla only. Writing each vanilla YMAP's version-to-version diffs is implemented but disabled (see [Limitations](reference.md#limitations)).

- [ ] Write source diffs against a selected or inferred vanilla version, not only the latest one.
- [ ] Add YBN diff documents and their TypeScript types.
- [ ] Implement `--get-diff SOURCE TARGET`; it currently only prints a mock invocation.
