use super::*;
use crate::{
  common::json::write_json,
  core::{
    game_archive::Staging,
    game_archive::{CacheVersion, VanillaCacheManifest},
  },
  platform::fs,
};
use std::collections::BTreeMap;

#[test]
fn generator_owns_missing_archive_generation_and_existing_archive_reuse() {
  let root = std::env::temp_dir().join(format!("vanilla_reuse_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let _cleanup = Staging(root.clone());
  let game_dir = root.join("game");
  let output_dir = root.join("vanilla");
  assert!(
    run_with_generation(&game_dir, &output_dir, true, || panic!("Missing game must fail first"))
      .unwrap_err()
      .to_string()
      .contains("specify --game-dir")
  );
  fs::create_dir_all(&game_dir).unwrap();
  let mut generated = false;
  run_with_generation(&game_dir, &output_dir, true, || {
    generated = true;
    fs::create_dir_all(&output_dir)?;
    write_json(
      &output_dir.join("cache_info.json"),
      &VanillaCacheManifest {
        format_version: 1,
        game_dir: game_dir.clone(),
        versions: vec![CacheVersion {
          id: "0000-base".into(),
          parent: None,
          archives: vec![],
          changes: BTreeMap::new(),
          unchanged: 0,
          file_kinds: Default::default(),
        }],
      },
    )
  })
  .unwrap();
  assert!(generated);
  fs::remove_dir_all(&game_dir).unwrap();
  run_with_generation(&game_dir, &output_dir, true, || {
    panic!("Existing archive must not generate")
  })
  .unwrap();
  fs::write(output_dir.join("cache_info.json"), b"invalid").unwrap();
  assert!(
    run_with_generation(&game_dir, &output_dir, true, || panic!(
      "Invalid archive must not generate"
    ))
    .is_err()
  );
  fs::remove_file(output_dir.join("cache_info.json")).unwrap();
  fs::write(output_dir.join("keep.txt"), b"keep").unwrap();
  assert!(
    run_with_generation(&game_dir, &output_dir, true, || {
      panic!("Nonempty archive must not generate")
    })
    .unwrap_err()
    .to_string()
    .contains("refusing automatic replacement")
  );
  let mut explicit = false;
  run_with_generation(&game_dir, &output_dir, false, || {
    explicit = true;
    Ok(())
  })
  .unwrap();
  assert!(explicit);
}
