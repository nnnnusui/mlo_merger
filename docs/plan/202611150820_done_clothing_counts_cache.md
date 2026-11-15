# Clothing Counts Cache

1. Add `--generate-vanilla-clothing-cache`, deriving per-stage clothing counts from the
   clothing archive into `asset/vanilla-clothing-cache`.
2. Parse each stage's `CPedVariationInfo` YMTs (RSC7 META and PSO) through `StructReader`,
   counting drawables and textures per component, and props and prop textures per anchor.
3. Record the archive manifest digest for freshness, publish through staging, and write
   `cache_info.json`, `summary.json` and per-stage `counts/{version}.json`.
4. Test counting, freshness and CLI parsing; update the clothing documentation chapters;
   run the command on the real archive.
