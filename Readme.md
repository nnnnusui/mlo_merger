
## Parse .ymap.xml

```
cargo run -- --parse-ymap-xml -i docs/sample/sample.ymap.xml
```

## Merge YMAP XML Files

Merge multiple mod YMAP XML files with vanilla files:

```
cargo run -- --merge-ymap-xml \
  --vanilla-dir asset/vanilla/ymap.xml \
  --mod-dir asset/merged/ymap.xml \
  --mod-ymap-dir asset/mlo/ymap.extracted \
  --output-dir asset/merged/ymap.xml
```

### Blacklist Configuration

You can filter out specific occlude models from being added during merge by using a blacklist configuration file:

```
cargo run -- --merge-ymap-xml \
  --vanilla-dir asset/vanilla/ymap.xml \
  --mod-dir asset/merged/ymap.xml \
  --mod-ymap-dir asset/mlo/ymap.extracted \
  --output-dir asset/merged/ymap.xml \
  --blacklist-config blacklist.toml
```

See `blacklist.toml` for configuration format.
