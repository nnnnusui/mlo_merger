//! The web demo's sample workspace under `web/sample`: an original vanilla archive and source
//! resources, built here so that no game data is shipped.
//!
//! Every name is invented (`smpl_*`, and DLCs `smpl_winter` and `smpl_summer`), and everything
//! sits around (-7100, 7100), in the sea off the map. The vanilla archive has the stages `base`,
//! `update`, `smpl_winter` (build 100 of the sample's `asset/config.toml`), a patch, and
//! `smpl_summer` (build 200), so `--gamebuild` changes what the merge sees:
//!
//! - `smpl_block_01.ymap` holds one item of every kind: entities with each extension (door,
//!   light effect, spawn point override, cloth bounds), a car generator, a time-cycle modifier,
//!   a box occluder, an occlude model, a physics dictionary and a grass batch. Its parent
//!   `smpl_block_01_lod.ymap` holds the LOD entities its houses point to.
//! - `winter_smpl_block_01.ymap` (build 100) and `summer_smpl_block_01.ymap` (build 200) ship it
//!   again with items reordered and added: DLC generations, which the vanilla cache pairs.
//! - `lodlights_small001.ymap` and `distlodlights_small001.ymap` are a LOD light pair, shipped
//!   again as `winter_*` with one more light.
//! - `smpl_block_02.ymap` changes in the title update, the patch and `smpl_summer`.
//! - `smpl_block_01.ybn` is the block's collision, changed in `smpl_summer`.
//!
//! The resources edit those files: two resources change `smpl_block_01.ymap` and its collision,
//! one removes a LOD entity from the parent so the children's parent indices are repaired, one
//! removes a LOD light pair, and one ships its own map and collision. Edits to
//! `smpl_block_01.ymap` carry over to its DLC generations.
//!
//! The test checks that `web/sample` holds what this module builds and that the pipeline runs
//! on it; run it with `MLO_WRITE_SAMPLES=1` to write the samples instead.

mod build;
#[cfg(test)]
mod tests;
mod ybn;

use build::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::core::format::gamefile::meta::jenk_hash;
use crate::core::format::ymap::binary::{write_ymap, ymap_to_model};
use crate::core::format::ymap::model::ymap::Ymap;
use crate::platform::fs;
use sha2::{Digest, Sha256};
use ybn::{Child, Geometry};

/// Guids of the sample's entities, so edits can find them.
mod guid {
  pub const HOUSE_A: u32 = 0x534D_0001;
  pub const HOUSE_B: u32 = 0x534D_0002;
  pub const DOOR: u32 = 0x534D_0003;
  pub const LAMP: u32 = 0x534D_0004;
  pub const BENCH: u32 = 0x534D_0005;
  pub const FLAG: u32 = 0x534D_0006;
  pub const BIN: u32 = 0x534D_0007;
  pub const XMAS_TREE: u32 = 0x534D_0008;
  pub const PARASOL: u32 = 0x534D_0009;
  pub const SLOD: u32 = 0x534D_0100;
  pub const HOUSE_A_LOD: u32 = 0x534D_0101;
  pub const HOUSE_B_LOD: u32 = 0x534D_0102;
  pub const OAK_1: u32 = 0x534D_0201;
  pub const OAK_2: u32 = 0x534D_0202;
  pub const OAK_3: u32 = 0x534D_0203;
  pub const FOUNTAIN: u32 = 0x534D_0204;
  pub const PARK_BENCH: u32 = 0x534D_0205;
  pub const KIOSK: u32 = 0x534D_0206;
  pub const PLANTER: u32 = 0x5A5A_0001;
  pub const BIRCH: u32 = 0x5A5A_0002;
  pub const SIGN: u32 = 0x5A5A_0101;
  pub const CRATE: u32 = 0x5A5A_0102;
}

/// One file of the sample workspace, by its path below the workspace.
struct SampleFile {
  path: String,
  bytes: Vec<u8>,
}

fn block_01_lod() -> Ymap {
  let mut map = ymap("smpl_block_01_lod", "", 2, 2);
  map.entities = vec![
    lod_entity(
      "smpl_block_slod",
      guid::SLOD,
      position(-7085.0, 7100.0, 20.0),
      "LODTYPES_DEPTH_SLOD1",
      0,
    ),
    lod_entity(
      "smpl_house_a_lod",
      guid::HOUSE_A_LOD,
      position(-7100.0, 7100.0, 20.0),
      "LODTYPES_DEPTH_LOD",
      1,
    ),
    lod_entity(
      "smpl_house_b_lod",
      guid::HOUSE_B_LOD,
      position(-7070.0, 7100.0, 20.0),
      "LODTYPES_DEPTH_LOD",
      1,
    ),
  ];
  fit_extents(&mut map);
  map
}

/// The block as the base game ships it, named `name` so DLC generations can reuse it.
fn block_01(name: &str) -> Ymap {
  let mut map = ymap(name, "smpl_block_01_lod", 0, 1);
  let mut house_a = entity("smpl_house_a", guid::HOUSE_A, position(-7100.0, 7100.0, 20.0), 0.0);
  house_a.parent_index = 1;
  let mut house_b = entity("smpl_house_b", guid::HOUSE_B, position(-7070.0, 7100.0, 20.0), 180.0);
  house_b.parent_index = 2;
  let mut door_entity = entity("smpl_door_front", guid::DOOR, position(-7100.0, 7092.0, 20.0), 0.0);
  door_entity.extensions.push(door("smpl_door_main", false));
  let mut lamp = entity("smpl_lamp_post", guid::LAMP, position(-7085.0, 7090.0, 20.0), 0.0);
  lamp.extensions.push(light_effect("smpl_lamp_light", [255, 214, 160]));
  let mut bench = entity("smpl_bench", guid::BENCH, position(-7080.0, 7088.0, 20.0), 90.0);
  bench.extensions.push(spawn_point("smpl_bench_seat"));
  let mut flag = entity("smpl_flag_cloth", guid::FLAG, position(-7060.0, 7092.0, 20.0), 0.0);
  flag.extensions.push(cloth_bounds("smpl_flag_bounds"));
  let bin = entity("smpl_bin", guid::BIN, position(-7078.0, 7088.0, 20.0), 0.0);
  map.entities = vec![house_a, house_b, door_entity, lamp, bench, flag, bin];
  map.car_generators.push(car_generator(position(-7090.0, 7080.0, 20.0), "smpl_hatchback"));
  map.time_cycle_modifiers.push(time_cycle_modifier(
    "smpl_dusk_glow",
    position(-7115.0, 7075.0, 15.0),
    position(-7055.0, 7115.0, 40.0),
    40.0,
  ));
  map.box_occluders.push(box_occluder(-7100.0, 7108.0, 24.0, [10.0, 8.0, 8.0]));
  map.occlude_models.push(occlude_model(-7074.0, 7104.0, 20.0));
  map.physics_dictionaries.push("smpl_props_phys".into());
  map.instanced_data.grass_instance_list.push(grass(
    "smpl_grass_tuft",
    position(-7110.0, 7082.0, 19.8),
    position(-7090.0, 7084.0, 20.2),
    6,
  ));
  fit_extents(&mut map);
  map
}

/// The block as `smpl_winter` ships it again: its items in another order, and a tree.
fn winter_block_01() -> Ymap {
  let mut map = block_01("winter_smpl_block_01");
  map.entities[2..].reverse();
  map.entities.push(entity(
    "smpl_xmas_tree",
    guid::XMAS_TREE,
    position(-7090.0, 7095.0, 20.0),
    0.0,
  ));
  fit_extents(&mut map);
  map
}

/// The block as `smpl_summer` ships it again: a parasol instead of the tree.
fn summer_block_01() -> Ymap {
  let mut map = block_01("summer_smpl_block_01");
  map.entities.push(entity("smpl_parasol", guid::PARASOL, position(-7076.0, 7086.0, 20.0), 0.0));
  fit_extents(&mut map);
  map
}

/// The park after `changes` of the stages that change it: `update`, the patch and
/// `smpl_summer`.
fn block_02(changes: usize) -> Ymap {
  let mut map = ymap("smpl_block_02", "", 0, 1);
  map.entities = vec![
    entity("smpl_tree_oak", guid::OAK_1, position(-7110.0, 7040.0, 20.0), 0.0),
    entity("smpl_tree_oak", guid::OAK_2, position(-7095.0, 7035.0, 20.0), 45.0),
    entity("smpl_tree_oak", guid::OAK_3, position(-7080.0, 7042.0, 20.0), 90.0),
    entity("smpl_fountain", guid::FOUNTAIN, position(-7095.0, 7050.0, 20.0), 0.0),
  ];
  if changes >= 1 {
    map.entities[0].position.x += 2.0;
  }
  if changes >= 2 {
    map.entities.push(entity(
      "smpl_bench",
      guid::PARK_BENCH,
      position(-7100.0, 7050.0, 20.0),
      270.0,
    ));
  }
  if changes >= 3 {
    map.entities.remove(1);
    map.entities.push(entity("smpl_kiosk", guid::KIOSK, position(-7085.0, 7055.0, 20.0), 180.0));
  }
  fit_extents(&mut map);
  map
}

/// LOD lights over the block's street, `count` of them, and their distant partners.
fn lod_lights(
  prefix: &str,
  count: usize,
) -> (Ymap, Ymap) {
  let places = [(-7085.0, 7090.0), (-7100.0, 7080.0), (-7070.0, 7080.0), (-7055.0, 7090.0)];
  let distant_name = format!("{prefix}distlodlights_small001");
  let mut lights = ymap(&format!("{prefix}lodlights_small001"), &distant_name, 0, 128);
  let mut distant = ymap(&distant_name, "", 2, 256);
  for (index, (x, y)) in places.into_iter().take(count).enumerate() {
    lights.lod_lights.push(lod_light(0x534D_1000 + index as u32));
    distant.distant_lod_lights.items.push(distant_light(position(x, y, 25.0), 0xFFD6_A0C8));
  }
  distant.distant_lod_lights.num_street_lights = count as u32;
  fit_extents(&mut distant);
  lights.streaming_extents_min = distant.streaming_extents_min.clone();
  lights.streaming_extents_max = distant.streaming_extents_max.clone();
  (lights, distant)
}

/// The block's collision: ground, house A and a bollard in one geometry, house B as a box.
/// `smpl_summer` adds the kiosk's collision.
fn block_01_collision(with_kiosk: bool) -> Vec<Child> {
  let mut ground = Geometry::ground([-7115.0, 7075.0], [-7055.0, 7115.0], 19.9);
  ground.add_box([-7106.0, 7096.0, 19.9], [-7094.0, 7110.0, 28.0]);
  ground.add_sphere([-7092.0, 7086.0, 20.4], 0.4);
  if with_kiosk {
    ground.add_box([-7088.0, 7076.0, 19.9], [-7084.0, 7079.0, 23.0]);
  }
  vec![Child::Geometry(ground), Child::Box([-7076.0, 7096.0, 19.9], [-7064.0, 7110.0, 28.0])]
}

/// Which entity of `map` has `guid`.
fn find(
  map: &Ymap,
  guid: u32,
) -> usize {
  map.entities.iter().position(|entity| entity.guid == guid).unwrap()
}

/// Resource `[smpl]/smpl_street_tweaks`: moves the bench, locks the door, removes the car
/// generator and adds a planter; removes a LOD light pair; adds the planter's collision.
fn street_tweaks() -> Vec<(&'static str, Vec<u8>)> {
  let mut map = block_01("smpl_block_01");
  let bench = find(&map, guid::BENCH);
  map.entities[bench].position.x += 1.5;
  let door_entity = find(&map, guid::DOOR);
  map.entities[door_entity].extensions = vec![door("smpl_door_main", true)];
  map.car_generators.clear();
  map.entities.push(entity("smpl_planter", guid::PLANTER, position(-7095.0, 7088.0, 20.0), 0.0));
  fit_extents(&mut map);
  let (mut lights, mut distant) = lod_lights("", 3);
  lights.lod_lights.remove(2);
  distant.distant_lod_lights.items.remove(2);
  distant.distant_lod_lights.num_street_lights -= 1;
  let mut collision = block_01_collision(false);
  if let Child::Geometry(ground) = &mut collision[0] {
    ground.add_box([-7096.0, 7087.0, 19.9], [-7094.0, 7089.0, 20.8]);
  }
  vec![
    ("stream/smpl_block_01.ymap", ymap_bytes(&map)),
    ("stream/lodlights_small001.ymap", ymap_bytes(&lights)),
    ("stream/distlodlights_small001.ymap", ymap_bytes(&distant)),
    ("stream/smpl_block_01.ybn", ybn_bytes(&collision)),
  ]
}

/// Resource `smpl_block_rework`: brightens the dusk, removes the occlude model's roof and the
/// bin, recolours the lamp; removes the empty SLOD from the parent, so the houses' parent
/// indices move; removes the bollard's collision.
fn block_rework() -> Vec<(&'static str, Vec<u8>)> {
  let mut map = block_01("smpl_block_01");
  map.time_cycle_modifiers[0].percentage = 80.0;
  map.occlude_models[0].triangles.truncate(2);
  map.entities.remove(find(&map, guid::BIN));
  let lamp = find(&map, guid::LAMP);
  map.entities[lamp].extensions = vec![light_effect("smpl_lamp_light", [170, 200, 255])];
  // The resource's own parent has lost its first entity, so its houses point one lower.
  let (house_a, house_b) = (find(&map, guid::HOUSE_A), find(&map, guid::HOUSE_B));
  map.entities[house_a].parent_index = 0;
  map.entities[house_b].parent_index = 1;
  fit_extents(&mut map);
  let mut parent = block_01_lod();
  parent.entities.remove(find(&parent, guid::SLOD));
  fit_extents(&mut parent);
  let mut collision = block_01_collision(false);
  if let Child::Geometry(ground) = &mut collision[0] {
    ground.vertices.pop();
    ground.polygons.pop();
  }
  vec![
    ("stream/maps/smpl_block_01.ymap", ymap_bytes(&map)),
    ("stream/maps/smpl_block_01_lod.ymap", ymap_bytes(&parent)),
    ("stream/maps/smpl_block_01.ybn", ybn_bytes(&collision)),
  ]
}

/// Resource `smpl_custom_props`: its own map and collision, and a birch in the park as
/// `smpl_summer` left it.
fn custom_props() -> Vec<(&'static str, Vec<u8>)> {
  let mut custom = ymap("smpl_custom_01", "", 0, 1);
  let mut sign = entity("smpl_sign_board", guid::SIGN, position(-7130.0, 7100.0, 20.0), 90.0);
  sign.extensions.push(light_effect("smpl_sign_light", [255, 80, 80]));
  custom.entities =
    vec![sign, entity("smpl_crate", guid::CRATE, position(-7128.0, 7094.0, 20.0), 15.0)];
  fit_extents(&mut custom);
  let mut park = block_02(3);
  park.entities.push(entity("smpl_tree_birch", guid::BIRCH, position(-7070.0, 7035.0, 20.0), 0.0));
  fit_extents(&mut park);
  let mut collision = Geometry::ground([-7135.0, 7090.0], [-7125.0, 7105.0], 19.9);
  collision.add_box([-7129.0, 7093.0, 19.9], [-7127.0, 7095.0, 21.0]);
  vec![
    ("stream/smpl_custom_01.ymap", ymap_bytes(&custom)),
    ("stream/smpl_custom_01.ybn", ybn_bytes(&[Child::Geometry(collision)])),
    ("stream/smpl_block_02.ymap", ymap_bytes(&park)),
  ]
}

/// Every name the sample hashes, so `hash_names.json` resolves them all.
const NAMES: &[&str] = &[
  "smpl_block_01",
  "smpl_block_01_lod",
  "winter_smpl_block_01",
  "summer_smpl_block_01",
  "smpl_block_02",
  "lodlights_small001",
  "distlodlights_small001",
  "winter_lodlights_small001",
  "winter_distlodlights_small001",
  "smpl_custom_01",
  "smpl_house_a",
  "smpl_house_b",
  "smpl_house_a_lod",
  "smpl_house_b_lod",
  "smpl_block_slod",
  "smpl_door_front",
  "smpl_lamp_post",
  "smpl_bench",
  "smpl_flag_cloth",
  "smpl_bin",
  "smpl_xmas_tree",
  "smpl_parasol",
  "smpl_tree_oak",
  "smpl_fountain",
  "smpl_kiosk",
  "smpl_planter",
  "smpl_tree_birch",
  "smpl_sign_board",
  "smpl_crate",
  "smpl_door_main",
  "smpl_door_audio",
  "smpl_lamp_light",
  "smpl_sign_light",
  "smpl_bench_seat",
  "smpl_sit_bench",
  "smpl_flag_bounds",
  "smpl_flag_pole",
  "smpl_hatchback",
  "smpl_dusk_glow",
  "smpl_props_phys",
  "smpl_grass_tuft",
];

fn hash_names() -> BTreeMap<u32, String> {
  NAMES.iter().map(|name| (jenk_hash(name), name.to_string())).collect()
}

/// The map's binary, checked to decode back to the same model with the sample's names.
fn ymap_bytes(map: &Ymap) -> Vec<u8> {
  let bytes = write_ymap(map, &map.entities).unwrap();
  let names = hash_names().into_iter().collect();
  assert_eq!(&ymap_to_model(&bytes, &names).unwrap(), map, "{} does not round-trip", map.name);
  bytes
}

fn ybn_bytes(children: &[Child]) -> Vec<u8> {
  crate::core::format::ybn::xml::xml_to_ybn(&ybn::xml(children)).unwrap()
}

type StageFiles = Vec<(&'static str, Vec<u8>)>;

/// The vanilla stages in load order, each with the files it ships.
fn vanilla_stages() -> Vec<(&'static str, StageFiles)> {
  let (lights, distant) = lod_lights("", 3);
  let (winter_lights, winter_distant) = lod_lights("winter_", 4);
  vec![
    (
      "0000-base",
      vec![
        ("smpl_block_01.ymap", ymap_bytes(&block_01("smpl_block_01"))),
        ("smpl_block_01_lod.ymap", ymap_bytes(&block_01_lod())),
        ("smpl_block_02.ymap", ymap_bytes(&block_02(0))),
        ("lodlights_small001.ymap", ymap_bytes(&lights)),
        ("distlodlights_small001.ymap", ymap_bytes(&distant)),
        ("smpl_block_01.ybn", ybn_bytes(&block_01_collision(false))),
      ],
    ),
    ("0001-update", vec![("smpl_block_02.ymap", ymap_bytes(&block_02(1)))]),
    (
      "0002-smpl_winter",
      vec![
        ("winter_smpl_block_01.ymap", ymap_bytes(&winter_block_01())),
        ("winter_lodlights_small001.ymap", ymap_bytes(&winter_lights)),
        ("winter_distlodlights_small001.ymap", ymap_bytes(&winter_distant)),
      ],
    ),
    ("0003-smpl_patch_01", vec![("smpl_block_02.ymap", ymap_bytes(&block_02(2)))]),
    (
      "0004-smpl_summer",
      vec![
        ("summer_smpl_block_01.ymap", ymap_bytes(&summer_block_01())),
        ("smpl_block_02.ymap", ymap_bytes(&block_02(3))),
        ("smpl_block_01.ybn", ybn_bytes(&block_01_collision(true))),
      ],
    ),
  ]
}

fn sha256(bytes: &[u8]) -> String {
  hex::encode(Sha256::digest(bytes))
}

const FXMANIFEST: &str = "fx_version 'cerulean'\ngame 'gta5'\n\nthis_is_a_map 'yes'\n";

/// The sample's configuration: default tolerances, and game builds placed by its invented DLCs.
const CONFIG: &str = "\
# The web sample's configuration. Its game builds are invented: build 100 first includes the
# smpl_winter DLC and build 200 smpl_summer; build 1 stops at the title update.
[gamebuild]
1 = []
100 = [\"smpl_winter\"]
200 = [\"smpl_summer\"]
";

/// Every file of the sample workspace, with `files.json` listing them for the page.
fn sample_files() -> Vec<SampleFile> {
  let mut files = Vec::new();
  let mut versions = Vec::new();
  let mut previous: BTreeMap<&str, String> = BTreeMap::new();
  let mut parent: Option<&str> = None;
  for (stage, stage_files) in vanilla_stages() {
    let mut changes = serde_json::Map::new();
    for (name, bytes) in stage_files {
      let extension = name.rsplit_once('.').unwrap().1;
      let object = format!("{stage}/{extension}/{name}");
      let sha = sha256(&bytes);
      changes.insert(
        name.into(),
        serde_json::json!({
          "previous_sha256": previous.get(name),
          "file": {"sha256": sha, "object": object, "source": format!("smpl.rpf/{name}")},
        }),
      );
      previous.insert(name, sha);
      files.push(SampleFile {
        path: format!("asset/vanilla/{object}"),
        bytes,
      });
    }
    versions.push(serde_json::json!({
      "id": stage, "parent": parent, "archives": ["smpl.rpf"], "changes": changes, "unchanged": 0,
    }));
    parent = Some(stage);
  }
  let manifest =
    serde_json::json!({"format_version": 1, "game_dir": "/mnt/gtav", "versions": versions});
  files.push(SampleFile {
    path: "asset/vanilla/cache_info.json".into(),
    bytes: serde_json::to_vec_pretty(&manifest).unwrap(),
  });
  let names = hash_names()
    .into_iter()
    .map(|(hash, name)| (format!("{hash:08X}"), name))
    .collect::<BTreeMap<_, _>>();
  files.push(SampleFile {
    path: "asset/vanilla/hash_names.json".into(),
    bytes: serde_json::to_vec_pretty(&serde_json::json!({"names": names})).unwrap(),
  });
  for (resource, resource_files) in [
    ("[smpl]/smpl_street_tweaks", street_tweaks()),
    ("smpl_block_rework", block_rework()),
    ("smpl_custom_props", custom_props()),
  ] {
    files.push(SampleFile {
      path: format!("asset/source/{resource}/fxmanifest.lua"),
      bytes: FXMANIFEST.into(),
    });
    for (path, bytes) in resource_files {
      files.push(SampleFile {
        path: format!("asset/source/{resource}/{path}"),
        bytes,
      });
    }
  }
  files.push(SampleFile {
    path: "asset/config.toml".into(),
    bytes: CONFIG.into(),
  });
  // The files the page seeds.
  let mut listed = files.iter().map(|file| file.path.clone()).collect::<Vec<_>>();
  listed.sort();
  files.push(SampleFile {
    path: "files.json".into(),
    bytes: serde_json::to_vec_pretty(&listed).unwrap(),
  });
  files
}

fn sample_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("web/sample")
}

/// Every file below `root` other than the hand-written `README.md`, relative to it.
fn existing_files(root: &Path) -> BTreeSet<String> {
  fs::WalkDir::new(root)
    .into_iter()
    .filter_map(Result::ok)
    .filter(|entry| entry.file_type().is_file())
    .map(|entry| entry.path().strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
    .filter(|path| path != "README.md")
    .collect()
}
