//! The sample's vanilla archive: its maps and collisions, the stages shipping them, the archives
//! holding them and what each DLC pack declares about those archives.
//!
//! The archives sit below `levels/smpl/` of their device, and the game loads them as follows:
//!
//! - `smpl_block.rpf`, the blocks, their LOD maps and collisions: always; `smpl_patch_01` and
//!   `smpl_summer` mount theirs over it.
//! - `smpl_lights.rpf`, the LOD light pair: always.
//! - `smpl_square.rpf`, `smpl_square.ymap`: until `smpl_winter` replaces it with its own, holding
//!   `winter_smpl_square.ymap`.
//! - `smpl_pier.rpf`, `smpl_pier.ymap`: until `smpl_winter` invalidates it, with nothing in its
//!   place.
//! - `smpl_yard.rpf`, `smpl_yard.ybn`: until `smpl_summer` replaces it with its own, holding the
//!   yard split in two.
//! - `smpl_winter.rpf` and `smpl_summer.rpf`, the packs' generations of the block and the lights,
//!   and the rink: from their pack on.
//! - `smpl_hut.rpf`, `winter_smpl_hut.ymap` in the installed game's patch of `smpl_winter`: never,
//!   as the pack does not declare it.

use super::build::*;
use super::guid;
use super::ybn::{Child, Geometry};
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::game_archive::dlc_content::{DlcChangeSet, DlcContent, DlcDataFile};

/// One file of a stage: its name, the archive holding it (its game path) and its bytes.
pub(super) type StageFile = (&'static str, String, Vec<u8>);

/// One vanilla stage, in load order.
pub(super) struct Stage {
  pub(super) label: &'static str,
  /// The archives the stage reads, as the manifest lists them.
  pub(super) archives: Vec<String>,
  pub(super) files: Vec<StageFile>,
  pub(super) dlc: Option<DlcContent>,
  pub(super) patches: Vec<Patch>,
}

/// One version of the title update's patch of a DLC pack.
pub(super) struct Patch {
  /// Its directory below `{stage}/update_rpf/`: the oldest game build shipping it.
  pub(super) name: &'static str,
  pub(super) builds: Vec<u32>,
  /// Whether the installed game, which the sample takes to be build 200, ships it.
  pub(super) installed: bool,
  pub(super) files: Vec<StageFile>,
}

/// The title update each game build's patches were read from.
pub(super) const TITLE_UPDATES: [(u32, &str); 2] =
  [(100, "update+update.rpf_5a4d5031"), (200, "update/update.rpf")];

fn base(archive: &str) -> String {
  format!("x64s.rpf/levels/smpl/{archive}.rpf")
}

fn update(archive: &str) -> String {
  format!("update/update.rpf/x64/levels/smpl/{archive}.rpf")
}

fn pack(
  pack: &str,
  archive: &str,
) -> String {
  format!("update/x64/dlcpacks/{pack}/dlc.rpf/x64/levels/smpl/{archive}.rpf")
}

fn patch(
  pack: &str,
  archive: &str,
) -> String {
  format!("update/update.rpf/dlc_patch/{pack}/x64/levels/smpl/{archive}.rpf")
}

/// A pack's archive as its `content.xml` names it.
fn device_path(
  device: &str,
  archive: &str,
) -> String {
  format!("{device}:/levels/smpl/{archive}.rpf")
}

fn data_file(
  path: String,
  overlay: bool,
  disabled: bool,
) -> DlcDataFile {
  DlcDataFile {
    path,
    overlay,
    disabled,
    file_type: "RPF_FILE".into(),
  }
}

/// `smpl_winter`: its own archive mounted as it is; its `smpl_square.rpf` enabled in place of the
/// base game's, and the pier's archive invalidated.
fn winter_content() -> DlcContent {
  let device = "dlcsmplwinter";
  DlcContent {
    device: device.into(),
    data_files: vec![
      data_file(device_path(device, "smpl_winter"), false, false),
      data_file(device_path(device, "smpl_square"), false, true),
    ],
    change_sets: vec![DlcChangeSet {
      name: "SMPL_WINTER_MAP".into(),
      invalidate: vec![
        device_path("platform", "smpl_square"),
        device_path("platform", "smpl_pier"),
      ],
      disable: vec![],
      enable: vec![device_path(device, "smpl_square")],
    }],
    ..DlcContent::default()
  }
}

/// `smpl_patch_01`: its `smpl_block.rpf` mounted over the base game's.
fn patch_01_content() -> DlcContent {
  let device = "dlcsmplpatch01";
  DlcContent {
    device: device.into(),
    data_files: vec![data_file(device_path(device, "smpl_block"), true, true)],
    change_sets: vec![DlcChangeSet {
      name: "SMPL_PATCH_01_MAP".into(),
      enable: vec![device_path(device, "smpl_block")],
      ..DlcChangeSet::default()
    }],
    ..DlcContent::default()
  }
}

/// `smpl_summer`: its own archive, its `smpl_block.rpf` mounted over the base game's, and its
/// `smpl_yard.rpf` enabled in place of the base game's.
fn summer_content() -> DlcContent {
  let device = "dlcsmplsummer";
  DlcContent {
    device: device.into(),
    data_files: vec![
      data_file(device_path(device, "smpl_summer"), false, false),
      data_file(device_path(device, "smpl_block"), true, true),
      data_file(device_path(device, "smpl_yard"), false, true),
    ],
    change_sets: vec![DlcChangeSet {
      name: "SMPL_SUMMER_MAP".into(),
      invalidate: vec![device_path("platform", "smpl_yard")],
      disable: vec![],
      enable: vec![device_path(device, "smpl_block"), device_path(device, "smpl_yard")],
    }],
    ..DlcContent::default()
  }
}

/// The vanilla stages in load order, each with the files it ships.
pub(super) fn stages() -> Vec<Stage> {
  let ymap = |map: &Ymap| super::ymap_bytes(map);
  let ybn = |children: &[Child]| super::ybn_bytes(children);
  let (lights, distant) = lod_lights("", 3);
  let (update_lights, update_distant) = lod_lights("", 4);
  let (winter_lights, winter_distant) = lod_lights("winter_", 5);
  let winter = |name, bytes| (name, pack("smpl_winter", "smpl_winter"), bytes);
  let summer = |name, archive, bytes| (name, pack("smpl_summer", archive), bytes);
  let winter_patch = |version| {
    let mut files =
      vec![("winter_smpl_rink.ymap", patch("smpl_winter", "smpl_winter"), ymap(&rink(version)))];
    if version == 2 {
      files.push(("winter_smpl_hut.ymap", patch("smpl_winter", "smpl_hut"), ymap(&hut())));
    }
    files
  };
  vec![
    Stage {
      label: "base",
      archives: vec!["x64s.rpf".into()],
      files: vec![
        ("smpl_block_01.ymap", base("smpl_block"), ymap(&block_01("smpl_block_01"))),
        ("smpl_block_01_lod.ymap", base("smpl_block"), ymap(&block_01_lod())),
        ("smpl_block_02.ymap", base("smpl_block"), ymap(&block_02(0))),
        ("smpl_square_lod.ymap", base("smpl_block"), ymap(&square_lod())),
        ("smpl_row.ymap", base("smpl_block"), ymap(&row())),
        ("smpl_row_lod.ymap", base("smpl_block"), ymap(&row_lod())),
        ("smpl_quay.ymap", base("smpl_block"), ymap(&quay())),
        ("smpl_quay_lod.ymap", base("smpl_block"), ymap(&quay_lod())),
        ("smpl_block_01.ybn", base("smpl_block"), ybn(&block_01_collision(false))),
        ("smpl_block_02.ybn", base("smpl_block"), ybn(&block_02_collision())),
        ("lodlights_small001.ymap", base("smpl_lights"), ymap(&lights)),
        ("distlodlights_small001.ymap", base("smpl_lights"), ymap(&distant)),
        ("smpl_square.ymap", base("smpl_square"), ymap(&square())),
        ("smpl_pier.ymap", base("smpl_pier"), ymap(&pier())),
        ("smpl_yard.ybn", base("smpl_yard"), ybn(&yard_collision(Yard::Base))),
      ],
      dlc: None,
      patches: vec![],
    },
    Stage {
      label: "update",
      archives: vec!["update/update.rpf".into()],
      files: vec![
        ("smpl_block_02.ymap", update("smpl_block"), ymap(&block_02(1))),
        ("lodlights_small001.ymap", update("smpl_lights"), ymap(&update_lights)),
        ("distlodlights_small001.ymap", update("smpl_lights"), ymap(&update_distant)),
        ("smpl_yard.ybn", update("smpl_yard"), ybn(&yard_collision(Yard::Update))),
      ],
      dlc: None,
      patches: vec![],
    },
    Stage {
      label: "smpl_winter",
      archives: vec!["update/x64/dlcpacks/smpl_winter/dlc.rpf".into()],
      files: vec![
        winter("winter_smpl_block_01.ymap", ymap(&winter_block_01())),
        winter("winter_lodlights_small001.ymap", ymap(&winter_lights)),
        winter("winter_distlodlights_small001.ymap", ymap(&winter_distant)),
        winter("winter_smpl_rink.ymap", ymap(&rink(0))),
        ("winter_smpl_square.ymap", pack("smpl_winter", "smpl_square"), ymap(&winter_square())),
      ],
      dlc: Some(winter_content()),
      patches: vec![
        Patch {
          name: "100",
          builds: vec![100],
          installed: false,
          files: winter_patch(1),
        },
        Patch {
          name: "200",
          builds: vec![200],
          installed: true,
          files: winter_patch(2),
        },
      ],
    },
    Stage {
      label: "smpl_patch_01",
      archives: vec!["update/x64/dlcpacks/smpl_patch_01/dlc.rpf".into()],
      files: vec![("smpl_block_02.ymap", pack("smpl_patch_01", "smpl_block"), ymap(&block_02(2)))],
      dlc: Some(patch_01_content()),
      patches: vec![],
    },
    Stage {
      label: "smpl_summer",
      archives: vec!["update/x64/dlcpacks/smpl_summer/dlc.rpf".into()],
      files: vec![
        summer("summer_smpl_block_01.ymap", "smpl_summer", ymap(&summer_block_01())),
        summer("summer_smpl_block_01.ybn", "smpl_summer", ybn(&block_01_collision(true))),
        summer("smpl_block_02.ymap", "smpl_block", ymap(&block_02(3))),
        summer("smpl_block_01.ybn", "smpl_block", ybn(&block_01_collision(true))),
        summer("summer_smpl_yard_0.ybn", "smpl_yard", ybn(&yard_collision(Yard::West))),
        summer("summer_smpl_yard_1.ybn", "smpl_yard", ybn(&yard_collision(Yard::East))),
      ],
      dlc: Some(summer_content()),
      patches: vec![],
    },
  ]
}

pub(super) fn block_01_lod() -> Ymap {
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
pub(super) fn block_01(name: &str) -> Ymap {
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
pub(super) fn summer_block_01() -> Ymap {
  let mut map = block_01("summer_smpl_block_01");
  map.entities.push(entity("smpl_parasol", guid::PARASOL, position(-7076.0, 7086.0, 20.0), 0.0));
  fit_extents(&mut map);
  map
}

/// The park after `changes` of the stages that change it: `update` moves the first oak and turns
/// the second, `smpl_patch_01` adds a bench and `smpl_summer` swaps the second oak for a kiosk.
pub(super) fn block_02(changes: usize) -> Ymap {
  let mut map = ymap("smpl_block_02", "", 0, 1);
  map.entities = vec![
    entity("smpl_tree_oak", guid::OAK_1, position(-7110.0, 7080.0, 20.0), 0.0),
    entity("smpl_tree_oak", guid::OAK_2, position(-7095.0, 7075.0, 20.0), 45.0),
    entity("smpl_tree_oak", guid::OAK_3, position(-7080.0, 7082.0, 20.0), 90.0),
    entity("smpl_fountain", guid::FOUNTAIN, position(-7095.0, 7090.0, 20.0), 0.0),
  ];
  if changes >= 1 {
    map.entities[0].position.x += 2.0;
    map.entities[1].rotation = heading(60.0);
  }
  if changes >= 2 {
    map.entities.push(entity(
      "smpl_bench",
      guid::PARK_BENCH,
      position(-7100.0, 7090.0, 20.0),
      270.0,
    ));
  }
  if changes >= 3 {
    map.entities.remove(1);
    map.entities.push(entity("smpl_kiosk", guid::KIOSK, position(-7085.0, 7095.0, 20.0), 180.0));
  }
  fit_extents(&mut map);
  map
}

/// Where the LOD lights over the block's street stand: the base game's three street lights, and
/// the title update's fourth and `smpl_winter`'s fifth, which are not street lights.
const LIGHT_PLACES: [(f32, f32); 5] =
  [(-7085.0, 7090.0), (-7100.0, 7080.0), (-7070.0, 7080.0), (-7055.0, 7090.0), (-7110.0, 7096.0)];

/// LOD lights over the block's street, `count` of them, and their distant partners.
pub(super) fn lod_lights(
  prefix: &str,
  count: usize,
) -> (Ymap, Ymap) {
  let distant_name = format!("{prefix}distlodlights_small001");
  let mut lights = ymap(&format!("{prefix}lodlights_small001"), &distant_name, 0, 128);
  let mut distant = ymap(&distant_name, "", 2, 256);
  for (index, (x, y)) in LIGHT_PLACES.into_iter().take(count).enumerate() {
    lights.lod_lights.push(lod_light(0x534D_1000 + index as u32));
    distant.distant_lod_lights.items.push(distant_light(position(x, y, 25.0), 0xFFD6_A0C8));
  }
  distant.distant_lod_lights.num_street_lights = count.min(3) as u32;
  fit_lights(&mut lights, &mut distant);
  (lights, distant)
}

/// Sets the extents of a LOD light pair from its distant lights.
pub(super) fn fit_lights(
  lights: &mut Ymap,
  distant: &mut Ymap,
) {
  fit_extents(distant);
  lights.streaming_extents_min = distant.streaming_extents_min.clone();
  lights.streaming_extents_max = distant.streaming_extents_max.clone();
  lights.entities_extents_min = distant.entities_extents_min.clone();
  lights.entities_extents_max = distant.entities_extents_max.clone();
}

/// The square's parent: the statue's LOD, which both generations of the square point to.
pub(super) fn square_lod() -> Ymap {
  let mut map = ymap("smpl_square_lod", "", 2, 2);
  map.entities = vec![lod_entity(
    "smpl_statue_lod",
    guid::STATUE_LOD,
    position(-7090.0, 7100.0, 20.0),
    "LODTYPES_DEPTH_LOD",
    1,
  )];
  fit_extents(&mut map);
  map
}

/// The square, in an archive of its own that `smpl_winter` replaces with its generation.
pub(super) fn square() -> Ymap {
  let mut map = ymap("smpl_square", "smpl_square_lod", 0, 1);
  let mut statue = entity("smpl_statue", guid::STATUE, position(-7090.0, 7100.0, 20.0), 0.0);
  statue.parent_index = 0;
  map.entities = vec![
    statue,
    entity("smpl_bench", guid::SQUARE_BENCH, position(-7096.0, 7088.0, 20.0), 0.0),
    entity("smpl_bin", guid::SQUARE_BIN, position(-7084.0, 7088.0, 20.0), 0.0),
    entity("smpl_tree_birch", guid::SQUARE_TREE, position(-7100.0, 7094.0, 20.0), 0.0),
    entity("smpl_lamp_post", guid::SQUARE_LAMP, position(-7080.0, 7094.0, 20.0), 0.0),
  ];
  fit_extents(&mut map);
  map
}

/// The square as `smpl_winter` ships it in its place: without the bench and the bin, and with a
/// snowman.
pub(super) fn winter_square() -> Ymap {
  let mut map = square();
  map.name = "winter_smpl_square".into();
  map.block.name = map.name.clone();
  map.entities.retain(|entity| ![guid::SQUARE_BENCH, guid::SQUARE_BIN].contains(&entity.guid));
  map.entities.push(entity("smpl_snowman", guid::SNOWMAN, position(-7092.0, 7086.0, 20.0), 0.0));
  fit_extents(&mut map);
  map
}

/// The pier, in an archive `smpl_winter` invalidates without replacing it.
fn pier() -> Ymap {
  let mut map = ymap("smpl_pier", "", 0, 1);
  map.entities = vec![
    entity("smpl_bollard", guid::PIER_BOLLARD_1, position(-7095.0, 7086.0, 20.0), 0.0),
    entity("smpl_bollard", guid::PIER_BOLLARD_2, position(-7085.0, 7086.0, 20.0), 0.0),
  ];
  fit_extents(&mut map);
  map
}

/// `smpl_winter`'s ice rink, which the title update's patch of the pack changes per game build:
/// `version` 1 (build 100) adds a lamp, and 2 (build 200) moves it and adds a bench.
pub(super) fn rink(version: usize) -> Ymap {
  let mut map = ymap("winter_smpl_rink", "", 0, 1);
  map.entities =
    vec![entity("smpl_rink_barrier", guid::RINK_BARRIER, position(-7092.0, 7094.0, 20.0), 0.0)];
  if version >= 1 {
    let x = if version >= 2 { -7084.0 } else { -7086.0 };
    map.entities.push(entity("smpl_lamp_post", guid::RINK_LAMP, position(x, 7090.0, 20.0), 0.0));
  }
  if version >= 2 {
    map.entities.push(entity("smpl_bench", guid::RINK_BENCH, position(-7096.0, 7090.0, 20.0), 0.0));
  }
  fit_extents(&mut map);
  map
}

/// A hut the installed game's patch of `smpl_winter` ships in an archive the pack does not
/// declare.
fn hut() -> Ymap {
  let mut map = ymap("winter_smpl_hut", "", 0, 1);
  map.entities = vec![entity("smpl_hut", guid::HUT, position(-7088.0, 7096.0, 20.0), 0.0)];
  fit_extents(&mut map);
  map
}

/// A row of houses: two under a SLOD, through their LOD entities, and two more whose LODs have no
/// parent.
pub(super) fn row() -> Ymap {
  let mut map = ymap("smpl_row", "smpl_row_lod", 0, 1);
  let house = |archetype, guid, x, parent| {
    let mut house = entity(archetype, guid, position(x, 7098.0, 20.0), 0.0);
    house.parent_index = parent;
    house
  };
  map.entities = vec![
    house("smpl_house_c", guid::HOUSE_C, -7100.0, 1),
    house("smpl_house_d", guid::HOUSE_D, -7090.0, 2),
    house("smpl_house_e", guid::HOUSE_E, -7080.0, 3),
    house("smpl_house_f", guid::HOUSE_F, -7070.0, 4),
  ];
  fit_extents(&mut map);
  map
}

/// The row's parent: a SLOD over the LODs of two houses, which point to it within the map, and
/// the other two houses' LODs.
pub(super) fn row_lod() -> Ymap {
  let mut map = ymap("smpl_row_lod", "", 2, 2);
  let lod = |archetype, guid, x| {
    let mut lod = lod_entity(archetype, guid, position(x, 7098.0, 20.0), "LODTYPES_DEPTH_LOD", 1);
    // Its parent, if any, is in this map.
    lod.flags &= !8;
    lod
  };
  let mut house_c = lod("smpl_house_c_lod", guid::HOUSE_C_LOD, -7100.0);
  house_c.parent_index = 0;
  let mut house_d = lod("smpl_house_d_lod", guid::HOUSE_D_LOD, -7090.0);
  house_d.parent_index = 0;
  map.entities = vec![
    lod_entity(
      "smpl_row_slod",
      guid::ROW_SLOD,
      position(-7095.0, 7098.0, 20.0),
      "LODTYPES_DEPTH_SLOD1",
      2,
    ),
    house_c,
    house_d,
    lod("smpl_house_e_lod", guid::HOUSE_E_LOD, -7080.0),
    lod("smpl_house_f_lod", guid::HOUSE_F_LOD, -7070.0),
  ];
  fit_extents(&mut map);
  map
}

/// The quay: a crane below the second of two LOD entities its parent gives one GUID.
pub(super) fn quay() -> Ymap {
  let mut map = ymap("smpl_quay", "smpl_quay_lod", 0, 1);
  let mut crane = entity("smpl_crane", guid::CRANE, position(-7088.0, 7090.0, 20.0), 0.0);
  crane.parent_index = 1;
  map.entities =
    vec![crane, entity("smpl_bollard", guid::QUAY_BOLLARD, position(-7094.0, 7088.0, 20.0), 0.0)];
  fit_extents(&mut map);
  map
}

/// The quay's parent, whose two crane LODs share a GUID, as a tool copying one may leave them.
fn quay_lod() -> Ymap {
  let mut map = ymap("smpl_quay_lod", "", 2, 2);
  map.entities = vec![
    lod_entity(
      "smpl_crane_lod",
      guid::CRANE_LOD,
      position(-7096.0, 7090.0, 20.0),
      "LODTYPES_DEPTH_LOD",
      0,
    ),
    lod_entity(
      "smpl_crane_lod",
      guid::CRANE_LOD,
      position(-7088.0, 7090.0, 20.0),
      "LODTYPES_DEPTH_LOD",
      1,
    ),
  ];
  fit_extents(&mut map);
  map
}

/// The block's collision: ground, house A and a bollard in one geometry, house B as a box.
/// `smpl_summer` adds the kiosk's collision.
pub(super) fn block_01_collision(with_kiosk: bool) -> Vec<Child> {
  let mut ground = Geometry::ground([-7115.0, 7075.0], [-7055.0, 7115.0], 19.9);
  ground.add_box([-7106.0, 7096.0, 19.9], [-7094.0, 7110.0, 28.0]);
  ground.add_sphere([-7092.0, 7086.0, 20.4], 0.4);
  if with_kiosk {
    ground.add_box([-7088.0, 7076.0, 19.9], [-7084.0, 7079.0, 23.0]);
  }
  vec![Child::Geometry(ground), Child::Box([-7076.0, 7096.0, 19.9], [-7064.0, 7110.0, 28.0])]
}

/// The park's collision, holding the fountain's box twice: no resource changes it, but the merge
/// deploys it without the repeat.
fn block_02_collision() -> Vec<Child> {
  let mut ground = Geometry::ground([-7100.0, 7070.0], [-7060.0, 7100.0], 19.9);
  for _ in 0..2 {
    ground.add_box([-7095.0, 7085.0, 19.9], [-7091.0, 7089.0, 20.8]);
  }
  vec![Child::Geometry(ground)]
}

/// Which part of the yard's collision a YBN holds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Yard {
  /// The base game's YBN.
  Base,
  /// The title update's, which adds a bench.
  Update,
  /// `smpl_summer`'s two YBNs splitting it, west and east; both hold the post on the split.
  West,
  East,
}

/// The yard's collision: ground, a shed and the title update's bench in the west, a post on the
/// line `smpl_summer` splits it along and a woodpile in the east.
pub(super) fn yard_collision(part: Yard) -> Vec<Child> {
  let (west, east) = match part {
    Yard::Base | Yard::Update => (-7115.0, -7065.0),
    Yard::West => (-7115.0, -7088.0),
    Yard::East => (-7092.0, -7065.0),
  };
  let mut ground = Geometry::ground([west, 7080.0], [east, 7104.0], 19.9);
  if part != Yard::East {
    ground.add_box([-7110.0, 7084.0, 19.9], [-7102.0, 7090.0, 23.0]);
  }
  if matches!(part, Yard::Update | Yard::West) {
    ground.add_box([-7108.0, 7094.0, 19.9], [-7104.0, 7095.0, 20.6]);
  }
  ground.add_box(YARD_POST.0, YARD_POST.1);
  if part != Yard::West {
    ground.add_sphere([-7075.0, 7095.0, 20.4], 0.5);
  }
  vec![Child::Geometry(ground)]
}

/// The yard's post, from one corner to the other.
pub(super) const YARD_POST: ([f32; 3], [f32; 3]) =
  ([-7091.0, 7091.0, 19.9], [-7089.0, 7093.0, 21.5]);

/// The sample's `materials.dat`: invented surface types in the layout of the game's, which the
/// title update ships; the sample's collision uses the second.
pub(super) fn materials_dat() -> String {
  let row = |name: &str, filter: &str, density: &str| {
    format!("{name:<20}\t{filter}\tFX_SMPL\tDEFAULT\t-\t-\t1.0\t0.1\t{density}\t1.0\r")
  };
  [
    "12.00\r".to_owned(),
    "# The web sample's surface types; the game's file has the same layout.\r".to_owned(),
    "# NAME\tFILTER\tFX\tAUDIO\tVFX_A\tVFX_B\tFRICTION\tELASTICITY\tDENSITY\tTYRE_GRIP\r"
      .to_owned(),
    row("SMPL_DEFAULT", "DEFAULT", "1000.0"),
    row("SMPL_PAVING", "STONE", "2400.0"),
    row("SMPL_LAWN", "GRASS", "1200.0"),
    row("SMPL_PLANKS", "WOOD", "700.0"),
    String::new(),
  ]
  .join("\n")
}
