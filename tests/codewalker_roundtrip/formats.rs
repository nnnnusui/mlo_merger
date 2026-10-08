use super::*;

#[test]
#[ignore]
fn native_ybn_ymt_ynd_ytyp_results_match_codewalker_where_supported() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let temp_dir =
    std::env::temp_dir().join(format!("mlo_merger_resource_diff_{}", std::process::id()));
  std::fs::create_dir_all(&temp_dir).unwrap();

  let cases = [
    (
      NativeResourceFormat::Ybn,
      base.join("asset/source/wxmaps_lshospital_v/stream/dt1_01_0.ybn"),
      "dt1_01_0.ybn",
    ),
    (
      NativeResourceFormat::YmtRsc,
      base.join("asset/source/[nteam]/cfx-nteam-acrt/SCENARIO/elysian_island.ymt"),
      "elysian_island.ymt",
    ),
    (
      NativeResourceFormat::Ynd,
      base.join("asset/source/[nteam]/cfx-nteam-road-connection/stream/ynd/nodes752.ynd"),
      "nodes752.ynd",
    ),
    (
      NativeResourceFormat::Ytyp,
      base.join("asset/source/sb_trainheistmap/stream/sb_train_addonprops.ytyp"),
      "sb_train_addonprops.ytyp",
    ),
  ];

  let mut failures = Vec::new();
  for (format, input, name) in cases {
    let result = compare_resource_with_codewalker(&codewalker, format, &input, name, &temp_dir);
    match result {
      Ok(()) => eprintln!("{}: Native XML and rebuilt binary match CodeWalker", input.display()),
      Err(message)
        if format == NativeResourceFormat::Ybn
          && message.contains("YBN Bounds resource graph adapter is not implemented") =>
      {
        eprintln!(
          "{}: CodeWalker export succeeded; Native conversion is explicitly unsupported",
          input.display()
        );
      }
      Err(message) => failures.push(format!("{}: {message}", input.display())),
    }
  }
  if failures.is_empty() {
    std::fs::remove_dir_all(&temp_dir).unwrap();
  } else {
    eprintln!("Preserving CodeWalker comparison artifacts under {}", temp_dir.display());
  }
  assert!(failures.is_empty(), "CodeWalker differential mismatches:\n{}", failures.join("\n"));
}
