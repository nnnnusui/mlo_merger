use super::*;
use crate::core::game_archive::Staging;
use std::collections::BTreeMap;

#[test]
fn generator_owns_missing_archive_generation_and_existing_archive_reuse() {
  let root = std::env::temp_dir().join(format!("vanilla_reuse_{}", std::process::id()));
  let _ = crate::platform::fs::remove_dir_all(&root);
  let _cleanup = Staging(root.clone());
  let mut request = GenerateVanilla {
    game_dir: root.join("game"),
    output_dir: root.join("vanilla"),
    magic: root.join("magic.dat"),
    reuse_existing: true,
  };
  assert!(
    request
      .run_with_generation(|| panic!("Missing game must fail first"))
      .unwrap_err()
      .to_string()
      .contains("specify --game-dir")
  );
  crate::platform::fs::create_dir_all(&request.game_dir).unwrap();
  let mut generated = false;
  request
    .run_with_generation(|| {
      generated = true;
      crate::platform::fs::create_dir_all(&request.output_dir)?;
      write_json(
        &request.output_dir.join("cache_info.json"),
        &VanillaCacheManifest {
          format_version: 1,
          game_dir: request.game_dir.clone(),
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
  crate::platform::fs::remove_dir_all(&request.game_dir).unwrap();
  request.run_with_generation(|| panic!("Existing archive must not generate")).unwrap();
  crate::platform::fs::write(request.output_dir.join("cache_info.json"), b"invalid").unwrap();
  assert!(request.run_with_generation(|| panic!("Invalid archive must not generate")).is_err());
  crate::platform::fs::remove_file(request.output_dir.join("cache_info.json")).unwrap();
  crate::platform::fs::write(request.output_dir.join("keep.txt"), b"keep").unwrap();
  assert!(
    request
      .run_with_generation(|| panic!("Nonempty archive must not generate"))
      .unwrap_err()
      .to_string()
      .contains("refusing automatic replacement")
  );
  request.reuse_existing = false;
  let mut explicit = false;
  request
    .run_with_generation(|| {
      explicit = true;
      Ok(())
    })
    .unwrap();
  assert!(explicit);
}
