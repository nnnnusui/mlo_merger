use std::collections::HashMap;

use super::{
  NativeResourceFormat, convert_files_from_xml, convert_files_to_xml, discover_schema_inputs,
  resource_to_xml, xml_to_resource,
};
use crate::core::format::gamefile::{
  meta_resource::{MetaResource, MetaSchemaCatalog},
  meta_xml::meta_to_xml,
  resource_file::Rsc7Resource,
};

fn assert_repeated_xml_conversion_is_stable(
  format: NativeResourceFormat,
  original_xml: &str,
  catalog: &MetaSchemaCatalog,
  label: &str,
) {
  let first_binary = xml_to_resource(format, original_xml, catalog).unwrap();
  let first_xml = resource_to_xml(format, &first_binary, &catalog.hash_names).unwrap();
  let second_binary = xml_to_resource(format, &first_xml, catalog).unwrap();
  let second_xml = resource_to_xml(format, &second_binary, &catalog.hash_names).unwrap();
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
    NativeResourceFormat::Ybn,
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
fn repeated_ybn_brofx_mansion_conversion_is_stable() {
  let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let binary =
    std::fs::read(base.join("asset/source/[patron]/brofx_mansion_06/stream/ch2_06_1.ybn")).unwrap();
  let xml = resource_to_xml(NativeResourceFormat::Ybn, &binary, &HashMap::new()).unwrap();
  assert_repeated_xml_conversion_is_stable(
    NativeResourceFormat::Ybn,
    &xml,
    &MetaSchemaCatalog::default(),
    "brofx_mansion_06/ch2_06_1.ybn",
  );
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
      NativeResourceFormat::Ymap,
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
  let source = base.join("asset/vanilla/ymap/cs1_railwyc.ymap");
  std::fs::copy(&source, input.join("nested/cs1_railwyc.ymap")).unwrap();
  assert_eq!(convert_files_to_xml(&input, &xml_dir).unwrap(), (1, 0));
  assert!(xml_dir.join("nested/cs1_railwyc.ymap.pso.xml").is_file());
  assert_eq!(
    NativeResourceFormat::from_xml_path(&xml_dir.join("nested/cs1_railwyc.ymap.pso.xml")).unwrap(),
    NativeResourceFormat::Ymap
  );
  assert!(convert_files_from_xml(&xml_dir, &rebuilt_dir, None).is_err());
  assert_eq!(convert_files_from_xml(&xml_dir, &rebuilt_dir, Some(&input)).unwrap(), (1, 0));
  let bytes = std::fs::read(rebuilt_dir.join("nested/cs1_railwyc.ymap")).unwrap();
  assert!(bytes.starts_with(b"PSIN"));
  let xml = resource_to_xml(NativeResourceFormat::Ymap, &bytes, &HashMap::new()).unwrap();
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

  assert_eq!(convert_files_from_xml(&input, &output, None).unwrap(), (1, 0));
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
fn ytyp_uses_shared_rsc_meta_conversion_path() {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/source/sb_trainheistmap/stream/sb_train_addonprops.ytyp");
  let bytes = std::fs::read(path).unwrap();
  let format = NativeResourceFormat::Ytyp;
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let meta = MetaResource::parse(&resource).unwrap();
  let xml = resource_to_xml(format, &bytes, &meta.hash_names()).unwrap();
  assert!(xml.contains("<CMapTypes"));

  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let rebuilt = xml_to_resource(format, &xml, &catalog).unwrap();
  let reparsed = MetaResource::parse(&Rsc7Resource::decode(&rebuilt).unwrap()).unwrap();
  assert_eq!(reparsed.root_block_index, meta.root_block_index);
  assert!(
    reparsed
      .structures
      .iter()
      .any(|schema| schema.name_hash == meta.data_blocks[0].structure_name_hash)
  );
  let rebuilt_xml = meta_to_xml(&reparsed, &catalog.hash_names).unwrap();
  assert!(rebuilt_xml.contains("<CMapTypes"));
}

#[test]
fn ybn_dispatch_rejects_invalid_resource_envelopes() {
  let error = resource_to_xml(NativeResourceFormat::Ybn, &[], &HashMap::new()).unwrap_err();
  assert!(error.to_string().contains("RSC7 header is truncated"));
}

#[test]
fn ybn_composite_geometry_bvh_fixture_round_trips_natively() {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/source/wxmaps_lshospital_v/stream/dt1_01_0.ybn");
  let bytes = std::fs::read(path).unwrap();
  let xml = resource_to_xml(NativeResourceFormat::Ybn, &bytes, &HashMap::new()).unwrap();
  assert!(xml.contains("<BoundsFile>"));
  assert_eq!(xml.matches("type=\"Composite\"").count(), 1);
  assert_eq!(xml.matches("type=\"GeometryBVH\"").count(), 3);
  assert!(xml.contains("<Box m=\""));

  let rebuilt =
    xml_to_resource(NativeResourceFormat::Ybn, &xml, &MetaSchemaCatalog::default()).unwrap();
  assert_eq!(&rebuilt[..4], b"RSC7");
  let rebuilt_xml = resource_to_xml(NativeResourceFormat::Ybn, &rebuilt, &HashMap::new()).unwrap();
  assert_eq!(rebuilt_xml.matches("type=\"GeometryBVH\"").count(), 3);
  assert!(rebuilt_xml.contains("<Box m=\""));
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
      xml_to_resource(NativeResourceFormat::Ybn, &xml, &MetaSchemaCatalog::default()).unwrap();
    let output = resource_to_xml(NativeResourceFormat::Ybn, &binary, &HashMap::new()).unwrap();
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
  let binary =
    xml_to_resource(NativeResourceFormat::Ybn, xml, &MetaSchemaCatalog::default()).unwrap();
  let output = resource_to_xml(NativeResourceFormat::Ybn, &binary, &HashMap::new()).unwrap();
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
  let binary =
    xml_to_resource(NativeResourceFormat::Ybn, xml, &MetaSchemaCatalog::default()).unwrap();
  let output = resource_to_xml(NativeResourceFormat::Ybn, &binary, &HashMap::new()).unwrap();
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
  let binary =
    xml_to_resource(NativeResourceFormat::Ynd, xml, &MetaSchemaCatalog::default()).unwrap();
  assert_eq!(&binary[..4], b"RSC7");
  let names = HashMap::from([(0x1234_ABCD, "test_street".to_string())]);
  let output = resource_to_xml(NativeResourceFormat::Ynd, &binary, &names).unwrap();
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
    resource_to_xml(NativeResourceFormat::YmtRsc, xml.as_bytes(), &HashMap::new()).unwrap(),
    xml
  );
  assert_eq!(
    xml_to_resource(NativeResourceFormat::YmtRsc, xml, &MetaSchemaCatalog::default()).unwrap(),
    xml.as_bytes()
  );
  let error =
    resource_to_xml(NativeResourceFormat::YmtRsc, b"PSIN\0\0\0\0", &HashMap::new()).unwrap_err();
  assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
  assert!(error.to_string().contains("PSO section length"));
}

#[test]
fn ymt_scenario_flags_use_codewalker_enum_names() {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/source/[nteam]/cfx-nteam-acrt/SCENARIO/elysian_island.ymt");
  let bytes = std::fs::read(path).unwrap();
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let meta = MetaResource::parse(&resource).unwrap();
  let xml = resource_to_xml(NativeResourceFormat::YmtRsc, &bytes, &meta.hash_names()).unwrap();

  assert!(xml.contains("<Flags>NoSpawn, FlyOffToOblivion, ExtendedRange</Flags>"));
  assert!(!xml.contains("hash_8A5F2D90"));
}

#[test]
fn batch_commands_convert_ytyp_xml_and_binary() {
  let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/source/sb_trainheistmap/stream/sb_train_addonprops.ytyp");
  let temp_dir = std::env::temp_dir().join(format!("native_ytyp_batch_{}", std::process::id()));
  let binary_input_dir = temp_dir.join("binary-input");
  let xml_output_dir = temp_dir.join("xml-output");
  let binary_output_dir = temp_dir.join("binary-output");
  std::fs::create_dir_all(&binary_input_dir).unwrap();
  std::fs::copy(&source, binary_input_dir.join(source.file_name().unwrap())).unwrap();

  assert_eq!(convert_files_to_xml(&binary_input_dir, &xml_output_dir).unwrap(), (1, 0));
  let xml_path = xml_output_dir.join("sb_train_addonprops.ytyp.xml");
  assert!(std::fs::read_to_string(&xml_path).unwrap().contains("<CMapTypes"));

  assert_eq!(
    convert_files_from_xml(&xml_output_dir, &binary_output_dir, Some(&binary_input_dir)).unwrap(),
    (1, 0)
  );
  let output_path = binary_output_dir.join("sb_train_addonprops.ytyp");
  let output = std::fs::read(output_path).unwrap();
  assert_eq!(&output[..4], b"RSC7");

  std::fs::remove_dir_all(temp_dir).unwrap();
}
