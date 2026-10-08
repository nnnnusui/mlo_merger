use super::*;

#[test]
#[ignore = "requires four local vanilla PSO YMAPs and CodeWalker.Core.dll"]
fn native_pso_ymap_exports_match_codewalker() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll").display().to_string()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let output = base.join("asset/merge_validation/pso_ymap");
  std::fs::create_dir_all(&output).unwrap();
  for name in ["cs1_railwyc", "cs1_railwyc_long_0", "id2_17", "id2_17_strm_0"] {
    let input = base.join(format!("asset/vanilla/ymap/{name}.ymap"));
    let reference = output.join(format!("{name}.ymap.codewalker.pso.xml"));
    codewalker.ymap_to_xml(&input, &reference).unwrap();
    let bytes = std::fs::read(&input).unwrap();
    let native = resource_to_xml(NativeResourceFormat::Ymap, &bytes, &Default::default()).unwrap();
    std::fs::write(output.join(format!("{name}.ymap.native.pso.xml")), &native).unwrap();
    let expected = std::fs::read_to_string(reference).unwrap();
    assert_canonical_xml_eq(&native, &expected, &format!("PSO export {name}")).unwrap();
    let rebuilt = mlo_merger::core::format::gamefile::pso::PsoResource::parse(&bytes)
      .unwrap()
      .rebuild_xml(&native)
      .unwrap();
    let rebuilt_path = output.join(format!("{name}.ymap"));
    std::fs::write(&rebuilt_path, rebuilt).unwrap();
    let rebuilt_xml = output.join(format!("{name}.rebuilt.pso.xml"));
    codewalker.ymap_to_xml(&rebuilt_path, &rebuilt_xml).unwrap();
    assert_canonical_xml_eq(
      &std::fs::read_to_string(rebuilt_xml).unwrap(),
      &expected,
      &format!("PSO rebuild {name}"),
    )
    .unwrap();
    eprintln!("{name}: Native PSO export matches CodeWalker");
  }
}
