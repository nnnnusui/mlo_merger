# Overwrite Sample

Overwrites leave chosen differences of a source YMAP out of the merge. The
sources are under `../source`, and their vanilla baseline is `../sample.ymap`
(vanilla `hei_ch1_11.ymap`).

- `resources/{resourceName}.json` holds the overwrites of one resource, keyed by the
  path below its `stream` directory. Use it as `asset/overwrite/resources/`.
- `diff/{resourceName}/{stream-relative-path}.diff.json` is the diff document the
  source cache writes for each source; the `skip` selectors pick their
  differences from its `diffs`. Paths are repository-relative and modification
  times are cleared.

`sample_resource_a` sets a content flag, moves and re-flags its first entity, and
removes the box occluder and triangles 10 and 11 of the occlude model. Its
overwrite leaves out everything except the new entity flags and the removal of
triangle 11:

- `"content_flags": true` drops a whole field.
- `{"Modified": {"guid": …, "changes": {"position": true}}}` drops only the
  position change of the entity with that GUID.
- `{"Removed": {"index": 0}}` drops the removal of vanilla box occluder 0.
- `changes.triangles` drops one nested triangle removal and keeps the other.

`sample_resource_b` adds an entity and removes triangle 20. Its overwrite drops
the added entity by its GUID (`{"Added": {"entity": {"guid": …}}}`).

`source` and `vanilla` record the files each overwrite was written against
(the `sha256` and `size` of the diff document's `source` and `vanilla`); an
overwrite whose files have changed is not applied.

These files are generated and checked by `core::overwrite::test::samples`; rerun it
with `MLO_WRITE_SAMPLES=1` after changing it.
