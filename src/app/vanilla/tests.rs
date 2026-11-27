use super::*;
use crate::{
  common::json::write_json,
  core::game_archive::{CacheVersion, Staging, VanillaCacheManifest},
  platform::fs,
};
use std::collections::BTreeMap;

type Contents = BTreeSet<ArchiveContent>;

fn write_archive(
  output_dir: &Path,
  game_dir: &Path,
  contents: Contents,
) -> Result<(), Box<dyn std::error::Error>> {
  fs::create_dir_all(output_dir)?;
  write_json(
    &output_dir.join("cache_info.json"),
    &VanillaCacheManifest {
      format_version: crate::core::game_archive::FORMAT_VERSION,
      contents,
      game_dir: game_dir.to_path_buf(),
      title_updates: Default::default(),
      versions: vec![CacheVersion {
        id: "0000-base".into(),
        parent: None,
        archives: vec![],
        changes: BTreeMap::new(),
        unchanged: 0,
        file_kinds: Default::default(),
        dlc: None,
        patches: Vec::new(),
      }],
    },
  )
}

#[test]
fn generator_owns_missing_archive_generation_and_existing_archive_reuse() {
  let root = std::env::temp_dir().join(format!("vanilla_reuse_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let _cleanup = Staging(root.clone());
  let game_dir = root.join("game");
  let output_dir = root.join("vanilla");
  let map: Contents = [ArchiveContent::Map].into();
  assert!(
    run_with_generation(&game_dir, &output_dir, &map, true, |_| panic!(
      "Missing game must fail first"
    ))
    .unwrap_err()
    .to_string()
    .contains("specify --game-dir")
  );
  fs::create_dir_all(&game_dir).unwrap();
  let mut generated = None;
  run_with_generation(&game_dir, &output_dir, &map, true, |contents| {
    write_archive(&output_dir, &game_dir, contents.clone())?;
    generated = Some(contents);
    Ok(())
  })
  .unwrap();
  assert_eq!(generated, Some(map.clone()));
  fs::remove_dir_all(&game_dir).unwrap();
  run_with_generation(&game_dir, &output_dir, &map, true, |_| {
    panic!("Existing archive must not generate")
  })
  .unwrap();
  assert!(
    run_with_generation(&game_dir, &output_dir, &[ArchiveContent::Clothing].into(), true, |_| {
      panic!("An archive without the content must not generate")
    })
    .unwrap_err()
    .to_string()
    .contains("--content clothing")
  );
  fs::write(output_dir.join("cache_info.json"), b"invalid").unwrap();
  assert!(
    run_with_generation(&game_dir, &output_dir, &map, true, |_| panic!(
      "Invalid archive must not generate"
    ))
    .is_err()
  );
  fs::remove_file(output_dir.join("cache_info.json")).unwrap();
  fs::write(output_dir.join("keep.txt"), b"keep").unwrap();
  assert!(
    run_with_generation(&game_dir, &output_dir, &map, true, |_| {
      panic!("Nonempty archive must not generate")
    })
    .unwrap_err()
    .to_string()
    .contains("refusing automatic replacement")
  );
  let mut explicit = false;
  run_with_generation(&game_dir, &output_dir, &map, false, |_| {
    explicit = true;
    Ok(())
  })
  .unwrap();
  assert!(explicit);
  assert!(
    run_with_generation(&game_dir, &output_dir, &Contents::new(), false, |_| panic!(
      "Nothing to extract"
    ))
    .is_err()
  );
}

#[test]
fn generation_keeps_the_content_the_existing_archive_holds() {
  let root = std::env::temp_dir().join(format!("vanilla_union_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let _cleanup = Staging(root.clone());
  let game_dir = root.join("game");
  let output_dir = root.join("vanilla");
  fs::create_dir_all(&game_dir).unwrap();
  write_archive(&output_dir, &game_dir, [ArchiveContent::Map].into()).unwrap();
  let mut extracted = None;
  run_with_generation(&game_dir, &output_dir, &[ArchiveContent::Clothing].into(), false, |c| {
    extracted = Some(c);
    Ok(())
  })
  .unwrap();
  assert_eq!(extracted, Some(ArchiveContent::ALL.into()));
}
