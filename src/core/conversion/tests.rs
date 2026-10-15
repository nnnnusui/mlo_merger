use std::collections::HashMap;

use super::{discover_schema_inputs, from_xml, to_xml};
use crate::core::format::gamefile::{meta::MetaSchemaCatalog, rsc7::Rsc7Resource};
use crate::core::format::stream_file::{FileType, StreamFile};

#[test]
fn managed_output_cleanup_preserves_rebuilt_and_unmanaged_files() {
  let temp =
    std::env::temp_dir().join(format!("mlo_managed_output_cleanup_{}", std::process::id()));
  let input = temp.join("input");
  let output = temp.join("output");
  std::fs::create_dir_all(&input).unwrap();
  std::fs::create_dir_all(&output).unwrap();
  std::fs::write(input.join("_managed_ymaps.txt"), "clone.ymap\nrebuilt.ymap\nvanilla.ymap\n")
    .unwrap();
  std::fs::write(input.join("rebuilt.ymap.xml"), "xml").unwrap();
  for name in ["clone.ymap", "rebuilt.ymap", "vanilla.ymap", "unmanaged.ymap"] {
    std::fs::write(output.join(name), "binary").unwrap();
  }
  let prune = || super::prune_managed_ymap_outputs(&input, &output);
  prune().unwrap();
  assert!(!output.join("clone.ymap").exists());
  assert!(!output.join("vanilla.ymap").exists());
  assert!(output.join("rebuilt.ymap").exists());
  assert!(output.join("unmanaged.ymap").exists());
  std::fs::write(input.join("_managed_ymaps.txt"), "unmanaged.ymap\n../escape.ymap\n").unwrap();
  assert!(prune().is_err());
  assert!(
    output.join("unmanaged.ymap").exists(),
    "validate the complete manifest before removing files"
  );
  std::fs::remove_dir_all(temp).unwrap();
}

fn assert_repeated_xml_conversion_is_stable(
  format: FileType,
  original_xml: &str,
  catalog: &MetaSchemaCatalog,
  label: &str,
) {
  let first_binary = StreamFile::encode_xml(format, original_xml, catalog).unwrap();
  let first_xml = StreamFile::decode_to_xml(format, &first_binary, &catalog.hash_names).unwrap();
  let second_binary = StreamFile::encode_xml(format, &first_xml, catalog).unwrap();
  let second_xml = StreamFile::decode_to_xml(format, &second_binary, &catalog.hash_names).unwrap();
  if first_binary != second_binary {
    let first = Rsc7Resource::decode(&first_binary).unwrap();
    let second = Rsc7Resource::decode(&second_binary).unwrap();
    let differences = first
      .system_data
      .iter()
      .zip(&second.system_data)
      .enumerate()
      .filter(|(_, (before, after))| before != after)
      .take(16)
      .map(|(offset, (before, after))| format!("{offset:#x}: {before:02x}->{after:02x}"))
      .collect::<Vec<_>>();
    eprintln!("{label}: first system-byte differences: {}", differences.join(", "));
  }
  if first_xml != second_xml
    && let Some((index, (before, after))) = first_xml
      .lines()
      .zip(second_xml.lines())
      .enumerate()
      .find(|(_, (before, after))| before != after)
  {
    eprintln!("{label}: first XML difference at line {}:\n{before}\n{after}", index + 1);
  }
  assert!(
    first_binary == second_binary && first_xml == second_xml,
    "repeated conversion changed: {label}; binary_equal={}, xml_equal={}, binary sizes {} -> {} bytes",
    first_binary == second_binary,
    first_xml == second_xml,
    first_binary.len(),
    second_binary.len()
  );
}

fn assert_ybn_fixture_conversion_is_stable(name: &str) {
  let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let path = base.join("docs/sample/ybn_conflicts").join(name);
  let xml = std::fs::read_to_string(&path).unwrap();
  assert_repeated_xml_conversion_is_stable(
    FileType::Ybn,
    &xml,
    &MetaSchemaCatalog::default(),
    name,
  );
}

#[test]
fn repeated_ybn_empty_composite_conversion_is_stable() {
  assert_ybn_fixture_conversion_is_stable("vanilla_empty.ybn.xml");
}

#[test]
fn repeated_ybn_resource_a_conversion_is_stable() {
  assert_ybn_fixture_conversion_is_stable("resource_a.ybn.xml");
}

#[test]
fn repeated_ybn_resource_b_conversion_is_stable() {
  assert_ybn_fixture_conversion_is_stable("resource_b.ybn.xml");
}

#[test]
fn repeated_ybn_geometry_bvh_conversion_is_stable() {
  assert_ybn_fixture_conversion_is_stable("geometry_bvh.ybn.xml");
}

#[test]
fn repeated_ymap_xml_binary_conversion_is_stable() {
  use crate::core::format::gamefile::test_support::{sample_ymap_catalog, sample_ymap_xml};
  for name in [
    "parent_refs/vanilla_parent.ymap.xml",
    "parent_refs/child.ymap.xml",
    "parent_refs/resource_a_parent.ymap.xml",
    "parent_refs/resource_b_parent.ymap.xml",
  ] {
    assert_repeated_xml_conversion_is_stable(
      FileType::Ymap,
      &sample_ymap_xml(name),
      sample_ymap_catalog(),
      name,
    );
  }
}

#[test]
fn pso_batch_commands_preserve_suffix_and_template_format() {
  let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let temp = std::env::temp_dir().join(format!("mlo_pso_batch_{}", std::process::id()));
  let input = temp.join("input");
  let xml_dir = temp.join("xml");
  let rebuilt_dir = temp.join("rebuilt");
  std::fs::create_dir_all(input.join("nested")).unwrap();
  let source = base.join("asset/vanilla/0000-base/ymap/cs1_railwyc.ymap");
  std::fs::copy(&source, input.join("nested/cs1_railwyc.ymap")).unwrap();
  assert_eq!(to_xml(&input, &xml_dir).unwrap(), (1, 0));
  assert!(xml_dir.join("nested/cs1_railwyc.ymap.pso.xml").is_file());
  assert_eq!(
    FileType::from_xml_path(&xml_dir.join("nested/cs1_railwyc.ymap.pso.xml")).unwrap(),
    FileType::Ymap
  );
  assert!(from_xml(&xml_dir, &rebuilt_dir, None).is_err());
  assert_eq!(from_xml(&xml_dir, &rebuilt_dir, Some(&input)).unwrap(), (1, 0));
  let bytes = std::fs::read(rebuilt_dir.join("nested/cs1_railwyc.ymap")).unwrap();
  assert!(bytes.starts_with(b"PSIN"));
  let xml = StreamFile::decode_to_xml(FileType::Ymap, &bytes, &HashMap::new()).unwrap();
  assert_eq!(
    xml,
    std::fs::read_to_string(xml_dir.join("nested/cs1_railwyc.ymap.pso.xml")).unwrap()
  );
  std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn ybn_xml_does_not_require_a_schema_directory() {
  let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let input = base.join("docs/sample/ybn_conflicts/resource_a.ybn.xml");
  let output = std::env::temp_dir().join(format!("ybn_from_xml_{}", std::process::id()));

  assert_eq!(from_xml(&input, &output, None).unwrap(), (1, 0));
  assert!(output.join("resource_a.ybn").is_file());
  std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn discovers_matching_vanilla_ymap_schema_for_source_xml() {
  let root = std::env::temp_dir().join(format!("ymap_schema_discovery_{}", std::process::id()));
  let input_dir = root.join("asset/source/resource/stream/ymap");
  let xml = input_dir.join("hei_sc1_18_strm_0.ymap.xml");
  let schema = root.join("asset/vanilla/ymap/hei_sc1_18_strm_0.ymap");
  std::fs::create_dir_all(&input_dir).unwrap();
  std::fs::create_dir_all(schema.parent().unwrap()).unwrap();
  std::fs::write(&xml, "<CMapData />").unwrap();
  std::fs::write(&schema, []).unwrap();

  let found = discover_schema_inputs(&xml, std::slice::from_ref(&xml)).unwrap();

  assert_eq!(found, vec![schema]);
  std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ybn_dispatch_rejects_invalid_resource_envelopes() {
  let error = StreamFile::decode_to_xml(FileType::Ybn, &[], &HashMap::new()).unwrap_err();
  assert!(error.to_string().contains("RSC7 header is truncated"));
}

#[test]
fn ybn_remaining_primitive_bounds_round_trip_natively() {
  for kind in ["Sphere", "Capsule", "Box", "Disc", "Cylinder", "Cloth"] {
    let xml = format!(
      r#"<BoundsFile><Bounds type="{kind}">
        <BoxMin x="-2" y="-2" z="-2" /><BoxMax x="2" y="2" z="2" />
        <BoxCenter x="0" y="0" z="0" /><SphereCenter x="0" y="0" z="0" />
        <SphereRadius value="2" /><Margin value="0.1" /><Volume value="1" />
        <Inertia x="0" y="0" z="0" /><MaterialIndex value="0" />
        <MaterialColourIndex value="0" /><ProceduralID value="0" />
        <RoomID value="0" /><PedDensity value="0" /><UnkFlags value="0" />
        <PolyFlags value="0" /><UnkType value="1" />
      </Bounds></BoundsFile>"#
    );
    let binary =
      StreamFile::encode_xml(FileType::Ybn, &xml, &MetaSchemaCatalog::default()).unwrap();
    let output = StreamFile::decode_to_xml(FileType::Ybn, &binary, &HashMap::new()).unwrap();
    assert!(output.contains(&format!("<Bounds type=\"{kind}\">")), "missing {kind}");
    assert!(output.contains("<SphereRadius value=\"2\""), "lost common fields for {kind}");
  }
}

#[test]
fn ybn_composite_none_child_round_trips_natively() {
  let xml = r#"<BoundsFile><Bounds type="Composite">
    <BoxMin x="0" y="0" z="0" /><BoxMax x="0" y="0" z="0" />
    <BoxCenter x="0" y="0" z="0" /><SphereCenter x="0" y="0" z="0" />
    <SphereRadius value="0" /><Margin value="0" /><Volume value="0" />
    <Inertia x="0" y="0" z="0" /><MaterialIndex value="0" />
    <MaterialColourIndex value="0" /><ProceduralID value="0" />
    <RoomID value="0" /><PedDensity value="0" /><UnkFlags value="0" />
    <PolyFlags value="0" /><UnkType value="1" />
    <Children><Item type="None" /></Children>
  </Bounds></BoundsFile>"#;
  let binary = StreamFile::encode_xml(FileType::Ybn, xml, &MetaSchemaCatalog::default()).unwrap();
  let output = StreamFile::decode_to_xml(FileType::Ybn, &binary, &HashMap::new()).unwrap();
  assert!(output.contains("<Item type=\"None\" />"));
}

#[test]
fn ybn_remaining_geometry_polygons_round_trip_natively() {
  let xml = r#"<BoundsFile><Bounds type="GeometryBVH">
    <BoxMin x="-2" y="-2" z="-2" /><BoxMax x="2" y="2" z="2" />
    <BoxCenter x="0" y="0" z="0" /><SphereCenter x="0" y="0" z="0" />
    <SphereRadius value="4" /><Margin value="0" /><Volume value="1" />
    <Inertia x="0" y="0" z="0" /><MaterialIndex value="0" />
    <MaterialColourIndex value="0" /><ProceduralID value="0" />
    <RoomID value="0" /><PedDensity value="0" /><UnkFlags value="0" />
    <PolyFlags value="0" /><UnkType value="1" />
    <GeometryCenter x="0" y="0" z="0" /><UnkFloat1 value="0" /><UnkFloat2 value="0" />
    <Materials><Item><Type value="1" /><ProceduralID value="0" /><RoomID value="0" />
      <PedDensity value="0" /><Flags>NONE</Flags><MaterialColourIndex value="0" /><Unk value="0" />
    </Item></Materials>
    <Vertices>0, 0, 0
      1, 0, 0
      0, 1, 0
      0, 0, 1</Vertices>
    <Polygons>
      <Sphere m="0" v="0" radius="0.5" />
      <Capsule m="0" v1="0" v2="1" radius="0.25" />
      <Cylinder m="0" v1="2" v2="3" radius="0.75" />
    </Polygons>
  </Bounds></BoundsFile>"#;
  let binary = StreamFile::encode_xml(FileType::Ybn, xml, &MetaSchemaCatalog::default()).unwrap();
  let output = StreamFile::decode_to_xml(FileType::Ybn, &binary, &HashMap::new()).unwrap();
  assert!(output.contains("<Sphere m=\"0\" v=\"0\" radius=\"0.5\""));
  assert!(output.contains("<Capsule m=\"0\" v1=\"0\" v2=\"1\" radius=\"0.25\""));
  assert!(output.contains("<Cylinder m=\"0\" v1=\"2\" v2=\"3\" radius=\"0.75\""));
}

#[test]
fn ynd_xml_round_trips_through_native_resource_codec() {
  let xml = r#"<NodeDictionary>
    <VehicleNodeCount value="1" />
    <PedNodeCount value="0" />
    <Nodes><Item>
      <AreaID value="7" /><NodeID value="0" /><StreetName>hash_1234ABCD</StreetName>
      <Position x="12.5" y="-8" z="3.25" />
      <Flags0 value="1" /><Flags1 value="2" /><Flags2 value="3" />
      <Flags3 value="4" /><Flags4 value="5" /><Flags5 value="1" />
      <Links><Item><ToAreaID value="8" /><ToNodeID value="9" />
        <Flags0 value="10" /><Flags1 value="11" /><Flags2 value="12" /><LinkLength value="13" />
      </Item></Links>
    </Item></Nodes>
    <Junctions><Item>
      <Position x="1.25" y="2.5" /><MinZ value="-1" /><MaxZ value="4" />
      <SizeX value="1" /><SizeY value="1" /><Heightmap>0x7F</Heightmap>
    </Item></Junctions>
    <JunctionRefs><Item>
      <AreaID value="7" /><NodeID value="0" /><JunctionID value="0" /><Unk0 value="0" />
    </Item></JunctionRefs>
  </NodeDictionary>"#;
  let binary = StreamFile::encode_xml(FileType::Ynd, xml, &MetaSchemaCatalog::default()).unwrap();
  assert_eq!(&binary[..4], b"RSC7");
  let names = HashMap::from([(0x1234_ABCD, "test_street".to_string())]);
  let output = StreamFile::decode_to_xml(FileType::Ynd, &binary, &names).unwrap();
  assert!(output.contains("<VehicleNodeCount value=\"1\""));
  assert!(output.contains("<StreetName>test_street</StreetName>"));
  assert!(output.contains("<LinkLength value=\"13\""));
  assert!(output.contains("<Heightmap>7F</Heightmap>"));
  assert!(output.contains("<JunctionID value=\"0\""));
}

#[test]
fn ymt_dispatch_preserves_text_xml_and_rejects_malformed_pso() {
  let xml = "<ScenarioManifest><Item /></ScenarioManifest>";
  assert_eq!(
    StreamFile::decode_to_xml(FileType::Ymt, xml.as_bytes(), &HashMap::new()).unwrap(),
    xml
  );
  assert_eq!(
    StreamFile::encode_xml(FileType::Ymt, xml, &MetaSchemaCatalog::default()).unwrap(),
    xml.as_bytes()
  );
  let error =
    StreamFile::decode_to_xml(FileType::Ymt, b"PSIN\0\0\0\0", &HashMap::new()).unwrap_err();
  assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
  assert!(error.to_string().contains("PSO section length"));
}
