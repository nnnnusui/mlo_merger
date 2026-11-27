# TODO

## Vanilla Archive (`asset/vanilla`)

- [ ] Record source `.rpf` paths with their size and modification time in the archive manifest, and treat an existing archive as stale when they change. Currently an existing archive is reused unless it is explicitly regenerated.

## DLC Generations

Merges deploy only the generation of each vanilla stream file the game loads and carry the changes resources make to the others over to it; see [Newer DLC Generations](reference.md#newer-dlc-generations).

- [ ] A pack's archive the title update patches without the pack declaring it (`not_declared` in `dlc_activity.json`) is taken as not loaded, and stands for no path, so the YBNs whose latest version it holds have no candidates to carry their changes to (93 on the local vanilla archive that a same-name successor used to take).
- [ ] Carry entity parent changes and additions with a parent to other generations; see the [plan](plan/202611191800_generation_entity_parents.md).
- [ ] Carry modifications of nested arrays to other generations, or list them in `left_behind.json`.

## Overwrites

Resource overwrites (`overwrite/resources/{resourceName}/{stream_path}.diff.json`) leave selected differences of a source stream file's diff out of the merge, or the file whole. The overwrite cache derives which resources' files each stream file that resources add is merged from, and the warnings. Vanilla overwrites (`overwrite/vanilla/{version}/ymap/*.diff.json`) add differences from the latest vanilla YMAP as the last merge source.

- [ ] Let a vanilla overwrite take precedence over resources' changes to the same items, if the merge policy gains configured priorities.
- [ ] Let a resource overwrite add differences of its own besides `skip`: `diffs` in a diff document's shape, adding or modifying items on top of the resource's diff, which is why resource overwrites are named `.diff.json`.
- [ ] Follow YBN overwrites to a new source version, as YMAP overwrites are followed; one written against an earlier version is only listed as `source_changed` now.
- [ ] Follow overwrites to a new vanilla version, as they are followed to a new source version: remap the vanilla indices their selectors name. Overwrites whose baseline changed are only listed in `ignored_overwrites` now.
- [ ] Report selectors that match no difference, unsupported or ambiguous overwrite changes, and overwrite files that are not read (such as the earlier `resources/{resourceName}.json` layout), in the overwrite cache's `warnings.json`, as `ignored_overwrites` does for missing and stale overwrites; they are only logged now.

## Deployment

- [ ] Deploy the merged files as a folder of resources split by area, file type and size, so that a change reloads only the resources holding it; merged files deployed for lighter collision (repeats left out of vanilla YBNs) make the single resource larger now. Choose with arguments between that folder, the single resource, or both, as use cases differ.

## Merge Policy

- [ ] Define the resource-priority configuration format, resource matching rules, and behavior for equal or unspecified priorities. Currently the first source that modifies or replaces an item wins, in source order.
- [ ] Merge overlapping resource edits by configured priority and record the selected source. Precedence must never depend on filesystem enumeration order.
- [ ] Report unsupported hierarchy/shape changes and conflicts for each format, and document the conservative fallback behavior.
- [ ] Remove a removed child's geometry from the LOD and SLOD models of its parents, which bake every child in; the merge updates the parents' `numChildren` and removes those left without children.

## Occlude Models

- [ ] Find what occlude model `flags` 1 does (see [the YMAP format notes](format/ymap.md#5-遮蔽用-ymap)). Test each case beside a control occluder known to work: horizontal planes facing up seen from above gave opposite results on two runs without one.

## YMAP Baseline Inference

- [-] Compare each source YMAP with the available vanilla versions and infer its source version from the number of differences.
- [-] Apply source changes from the inferred baseline to the latest vanilla YMAP and repair parent references.

## Diff Documents

Source-cache diff documents currently cover stream files against the latest vanilla only. Writing each vanilla YMAP's version-to-version diffs is implemented but disabled (see [Limitations](reference.md#limitations)).

- [ ] Write source diffs against a selected or inferred vanilla version, not only the latest one.
- [ ] Implement `--get-diff SOURCE TARGET`; it currently only prints a mock invocation.

## Clothing Collection (`asset/vanilla`)

- [x] Collect raw clothing and appearance variation YMTs for all vanilla and DLC Peds with `--generate-vanilla --content clothing`, in the shared versioned vanilla archive.
