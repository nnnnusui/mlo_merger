//! The sample's resources, each named after what it shows; see the README of `web/sample` for
//! the example each belongs to.

use super::build::*;
use super::vanilla::*;
use super::ybn::{Child, Geometry, Polygon};
use super::{find, guid, ybn_bytes, ymap_bytes};
use crate::core::format::ymap::model::ymap::Ymap;

/// A resource's files, by their path below the resource.
pub(super) type ResourceFiles = Vec<(&'static str, Vec<u8>)>;

/// Every resource, by its path below `source`, at its `earlier` version or its current one.
pub(super) fn resources(earlier: bool) -> Vec<(&'static str, ResourceFiles)> {
  vec![
    ("[smpl]/smpl_street_tweaks", street_tweaks(earlier)),
    ("smpl_block_rework", block_rework(earlier)),
    ("smpl_custom_props", custom_props()),
    ("smpl_custom_props_v2", custom_props_v2()),
    ("smpl_park_tweaks", park_tweaks(earlier)),
    ("smpl_summer_only", summer_only()),
    ("smpl_left_behind", left_behind()),
    ("smpl_yard_split", yard_split()),
    ("smpl_old_park", old_park()),
    ("smpl_old_lights", old_lights()),
    ("smpl_vanilla_rink", vanilla_rink()),
    ("smpl_houses_gone", houses_gone()),
    ("smpl_shared_parent_guid", shared_parent_guid()),
    ("smpl_orphaned_house", orphaned_house()),
    ("smpl_bench_moved", bench_moved(false)),
    ("smpl_bench_moved_too", bench_moved(true)),
    ("smpl_square_both", square_both()),
    ("smpl_rounded_guids", rounded_guids()),
    ("smpl_distant_only", distant_only()),
    ("smpl_one_side_light", one_side_light()),
  ]
}

/// Panels of the wall strip each of two resources adds to the block's occlude model, joining the
/// wall at either end: 142 vertices each, which together with the wall's pass the 255 one model
/// holds.
pub(super) const WALL_PANELS: usize = 70;

/// Resource `[smpl]/smpl_street_tweaks`: moves the bench, locks the door, removes the car
/// generator, adds a planter and extends the occlude model's wall east; removes the first light
/// of the LOD light pair as the title update left it; adds the planter's collision. Its
/// `earlier` version, which an overwrite was written against, also added a cone.
pub(super) fn street_tweaks(earlier: bool) -> ResourceFiles {
  let mut map = block_01("smpl_block_01");
  let bench = find(&map, guid::BENCH);
  map.entities[bench].position.x += 1.5;
  let door_entity = find(&map, guid::DOOR);
  map.entities[door_entity].extensions = vec![door("smpl_door_main", true)];
  map.car_generators.clear();
  let model = &mut map.occlude_models[0];
  model.triangles.extend(wall_strip(position(-7066.0, 7104.0, 20.0), WALL_PANELS));
  model.fit_bounds();
  map.entities.push(entity("smpl_planter", guid::PLANTER, position(-7095.0, 7088.0, 20.0), 0.0));
  if earlier {
    map.entities.push(entity("smpl_cone", guid::CONE, position(-7092.0, 7090.0, 20.0), 0.0));
  }
  fit_extents(&mut map);
  let (mut lights, mut distant) = lod_lights("", 4);
  lights.lod_lights.remove(0);
  distant.distant_lod_lights.items.remove(0);
  distant.distant_lod_lights.num_street_lights -= 1;
  fit_lights(&mut lights, &mut distant);
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

/// Resource `smpl_block_rework`: brightens the dusk, removes the occlude model's roof and extends
/// its wall west, removes the bin, recolours the lamp; removes the empty SLOD from the parent, so
/// the houses' parent indices move; removes the bollard's collision. Its `earlier` version,
/// which an overwrite was written against, recoloured the lamp otherwise.
pub(super) fn block_rework(earlier: bool) -> ResourceFiles {
  let mut map = block_01("smpl_block_01");
  map.time_cycle_modifiers[0].percentage = 80.0;
  let model = &mut map.occlude_models[0];
  model.triangles.truncate(2);
  model.triangles.extend(wall_strip(position(-7102.0, 7104.0, 20.0), WALL_PANELS));
  model.fit_bounds();
  map.entities.remove(find(&map, guid::BIN));
  let lamp = find(&map, guid::LAMP);
  let colour = if earlier { [200, 200, 255] } else { [170, 200, 255] };
  map.entities[lamp].extensions = vec![light_effect("smpl_lamp_light", colour)];
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

/// The custom map as `smpl_custom_props` ships it: a sign and a crate.
fn custom_01() -> Ymap {
  let mut custom = ymap("smpl_custom_01", "", 0, 1);
  let mut sign = entity("smpl_sign_board", guid::SIGN, position(-7094.0, 7097.0, 20.0), 90.0);
  sign.extensions.push(light_effect("smpl_sign_light", [255, 80, 80]));
  custom.entities =
    vec![sign, entity("smpl_crate", guid::CRATE, position(-7091.0, 7095.0, 20.0), 15.0)];
  fit_extents(&mut custom);
  custom
}

/// Resource `smpl_custom_props`: its own map, a child map whose label points at the map's crate,
/// the map's collision, a birch in the park as `smpl_summer` left it, and `smpl_summer`'s
/// generation of the block's collision with the kiosk's taken out and a bench's put in. Under
/// a game build before `smpl_summer`, those changes are carried to `smpl_block_01.ybn`.
fn custom_props() -> ResourceFiles {
  let custom = custom_01();
  let mut detail = ymap("smpl_custom_02", "smpl_custom_01", 0, 1);
  let mut label =
    entity("smpl_crate_label", guid::CRATE_LABEL, position(-7091.0, 7094.5, 20.5), 15.0);
  label.parent_index = find(&custom, guid::CRATE) as i32;
  detail.entities = vec![label];
  fit_extents(&mut detail);
  let mut park = block_02(3);
  park.entities.push(entity("smpl_tree_birch", guid::BIRCH, position(-7070.0, 7075.0, 20.0), 0.0));
  fit_extents(&mut park);
  let mut collision = Geometry::ground([-7096.0, 7090.0], [-7086.0, 7100.0], 19.9);
  collision.add_box([-7092.0, 7094.0, 19.9], [-7090.0, 7096.0, 21.0]);
  vec![
    ("stream/smpl_custom_01.ymap", ymap_bytes(&custom)),
    ("stream/smpl_custom_02.ymap", ymap_bytes(&detail)),
    ("stream/smpl_custom_01.ybn", ybn_bytes(&[Child::Geometry(collision)])),
    ("stream/smpl_block_02.ymap", ymap_bytes(&park)),
    ("stream/summer_smpl_block_01.ybn", ybn_bytes(&summer_collision_with_bench())),
  ]
}

/// The collision `smpl_summer` ships as `summer_smpl_block_01.ybn`, without the kiosk's box and
/// with a bench's.
fn summer_collision_with_bench() -> Vec<Child> {
  let mut collision = block_01_collision(false);
  if let Child::Geometry(ground) = &mut collision[0] {
    ground.add_box([-7082.0, 7087.0, 19.9], [-7080.0, 7088.0, 20.5]);
  }
  collision
}

/// Resource `smpl_custom_props_v2`, a later version of `smpl_custom_props` loaded after it: the
/// same custom map with a barrel first, the sign recoloured, and the crate placed again under
/// another GUID and turned.
fn custom_props_v2() -> ResourceFiles {
  let mut custom = custom_01();
  let sign = find(&custom, guid::SIGN);
  custom.entities[sign].extensions = vec![light_effect("smpl_sign_light", [80, 160, 255])];
  let mut crate_v2 = custom.entities.remove(find(&custom, guid::CRATE));
  crate_v2.guid = guid::CRATE_V2;
  crate_v2.rotation = heading(45.0);
  custom
    .entities
    .insert(0, entity("smpl_barrel", guid::BARREL, position(-7088.0, 7096.0, 20.0), 0.0));
  custom.entities.push(crate_v2);
  fit_extents(&mut custom);
  vec![("stream/smpl_custom_01.ymap", ymap_bytes(&custom))]
}

/// Resource `smpl_park_tweaks`: removes an oak from the park as `smpl_summer` left it and
/// places a bin by the fountain. Its `earlier` version, which an overwrite was written against,
/// removed the other oak and placed the bin last.
pub(super) fn park_tweaks(earlier: bool) -> ResourceFiles {
  let mut park = block_02(3);
  park.entities.remove(find(&park, if earlier { guid::OAK_1 } else { guid::OAK_3 }));
  let bin = entity("smpl_bin", guid::PARK_BIN, position(-7093.0, 7088.0, 20.0), 0.0);
  let at = if earlier { park.entities.len() } else { find(&park, guid::FOUNTAIN) + 1 };
  park.entities.insert(at, bin);
  fit_extents(&mut park);
  vec![("stream/smpl_block_02.ymap", ymap_bytes(&park))]
}

/// Resource `smpl_summer_only`: ships only `smpl_summer`'s generation of the block, turning the
/// flag and moving the parasol. Under build 100, which ends before `smpl_summer`, the flag's turn
/// is carried to the winter generation, and the parasol, which no older generation has, is left
/// behind.
fn summer_only() -> ResourceFiles {
  let mut map = summer_block_01();
  let flag = find(&map, guid::FLAG);
  map.entities[flag].rotation = heading(30.0);
  let parasol = find(&map, guid::PARASOL);
  map.entities[parasol].position.y += 2.0;
  fit_extents(&mut map);
  vec![("stream/summer_smpl_block_01.ymap", ymap_bytes(&map))]
}

/// Resource `smpl_left_behind`: changes the square, which the game no longer loads once
/// `smpl_winter` replaces it: moves the statue, which is carried; removes the bench and turns the
/// bin, which the winter square lacks; and adds a sign below the statue's LOD, an addition with a
/// parent, which is not carried.
fn left_behind() -> ResourceFiles {
  let mut map = square();
  let statue = find(&map, guid::STATUE);
  map.entities[statue].position.x += 1.0;
  map.entities.remove(find(&map, guid::SQUARE_BENCH));
  let bin = find(&map, guid::SQUARE_BIN);
  map.entities[bin].rotation = heading(45.0);
  let mut sign = entity("smpl_sign_board", guid::SQUARE_SIGN, position(-7090.0, 7096.0, 20.0), 0.0);
  sign.parent_index = 0;
  map.entities.push(sign);
  fit_extents(&mut map);
  vec![("stream/smpl_square.ymap", ymap_bytes(&map))]
}

/// Resource `smpl_yard_split`: made from the yard's collision before the title update's bench,
/// removes the post and adds a crate by the shed. Once `smpl_summer` splits the yard into two
/// YBNs, the post's removal goes to both, which hold it, and the crate to the western one, which
/// covers it; before, the resource's YBN is merged from its differences, keeping the bench.
fn yard_split() -> ResourceFiles {
  let mut collision = yard_collision(Yard::Base);
  if let Child::Geometry(ground) = &mut collision[0] {
    let post = ground
      .polygons
      .iter()
      .position(|polygon| {
        matches!(polygon, Polygon::Box([first, ..]) if ground.vertices[*first] == YARD_POST.0)
      })
      .unwrap();
    ground.polygons.remove(post);
    ground.add_box([-7100.0, 7086.0, 19.9], [-7098.0, 7088.0, 21.0]);
  }
  vec![("stream/smpl_yard.ybn", ybn_bytes(&collision))]
}

/// Resource `smpl_old_park`: made from the park as the title update left it, moving the first
/// oak further. It has no bench and keeps the second oak, which later stages add and swap for
/// the kiosk.
fn old_park() -> ResourceFiles {
  let mut park = block_02(1);
  let oak = find(&park, guid::OAK_1);
  park.entities[oak].position.y += 3.0;
  fit_extents(&mut park);
  vec![("stream/smpl_block_02.ymap", ymap_bytes(&park))]
}

/// Resource `smpl_old_lights`: the LOD light pair as the base game shipped it, before the title
/// update's fourth light, the third light removed from `lodlights_small001.ymap` only.
fn old_lights() -> ResourceFiles {
  let (mut lights, distant) = lod_lights("", 3);
  lights.lod_lights.remove(2);
  vec![
    ("stream/lodlights_small001.ymap", ymap_bytes(&lights)),
    ("stream/distlodlights_small001.ymap", ymap_bytes(&distant)),
  ]
}

/// Resource `smpl_vanilla_rink`: `smpl_winter`'s rink exactly as the pack shipped it, before
/// the title update's patches of it.
fn vanilla_rink() -> ResourceFiles {
  vec![("stream/winter_smpl_rink.ymap", ymap_bytes(&rink(0)))]
}

/// Resource `smpl_houses_gone`: removes the two houses of the row below its SLOD.
fn houses_gone() -> ResourceFiles {
  let mut map = row();
  map.entities.retain(|entity| ![guid::HOUSE_C, guid::HOUSE_D].contains(&entity.guid));
  fit_extents(&mut map);
  vec![("stream/smpl_row.ymap", ymap_bytes(&map))]
}

/// Resource `smpl_shared_parent_guid`: turns the quay's crane, whose parent shares its GUID with
/// another LOD entity.
fn shared_parent_guid() -> ResourceFiles {
  let mut map = quay();
  let crane = find(&map, guid::CRANE);
  map.entities[crane].rotation = heading(20.0);
  vec![("stream/smpl_quay.ymap", ymap_bytes(&map))]
}

/// Resource `smpl_orphaned_house`: removes the third house's LOD from the row's parent, leaving
/// the house without a parent; the fourth house then points one entity lower.
fn orphaned_house() -> ResourceFiles {
  let mut map = row_lod();
  map.entities.remove(find(&map, guid::HOUSE_E_LOD));
  fit_extents(&mut map);
  vec![("stream/smpl_row_lod.ymap", ymap_bytes(&map))]
}

/// Resources `smpl_bench_moved` and, `too`, `smpl_bench_moved_too`: both move the park's bench
/// alike and turn the fountain, each by its own angle; the second also turns the oak
/// `smpl_park_tweaks` removes.
fn bench_moved(too: bool) -> ResourceFiles {
  let mut park = block_02(3);
  let bench = find(&park, guid::PARK_BENCH);
  park.entities[bench].position.x += 1.0;
  let fountain = find(&park, guid::FOUNTAIN);
  park.entities[fountain].rotation = heading(if too { 20.0 } else { 10.0 });
  if too {
    let oak = find(&park, guid::OAK_3);
    park.entities[oak].rotation = heading(120.0);
  }
  vec![("stream/smpl_block_02.ymap", ymap_bytes(&park))]
}

/// Resource `smpl_square_both`: ships both generations of the square, turning the birch in each
/// by its own angle and planting the same flower bed in each.
fn square_both() -> ResourceFiles {
  let edit = |mut map: Ymap, degrees: f32| {
    let tree = find(&map, guid::SQUARE_TREE);
    map.entities[tree].rotation = heading(degrees);
    map.entities.push(entity(
      "smpl_flower_bed",
      guid::FLOWER_BED,
      position(-7086.0, 7098.0, 20.0),
      0.0,
    ));
    fit_extents(&mut map);
    ymap_bytes(&map)
  };
  vec![
    ("stream/smpl_square.ymap", edit(square(), 30.0)),
    ("stream/winter_smpl_square.ymap", edit(winter_square(), 60.0)),
  ]
}

/// Resource `smpl_rounded_guids`: the park as `smpl_summer` left it, saved by a tool passing
/// GUIDs through a 32-bit float, with the kiosk turned.
fn rounded_guids() -> ResourceFiles {
  let mut park = block_02(3);
  for entity in &mut park.entities {
    entity.guid = entity.guid as f32 as u32;
  }
  let kiosk = park.entities.iter().position(|entity| entity.archetype_name == "smpl_kiosk");
  park.entities[kiosk.unwrap()].rotation = heading(200.0);
  vec![("stream/smpl_block_02.ymap", ymap_bytes(&park))]
}

/// Resource `smpl_distant_only`: ships only `distlodlights_small001.ymap`, without its second
/// light.
fn distant_only() -> ResourceFiles {
  let (mut lights, mut distant) = lod_lights("", 4);
  distant.distant_lod_lights.items.remove(1);
  distant.distant_lod_lights.num_street_lights -= 1;
  fit_lights(&mut lights, &mut distant);
  vec![("stream/distlodlights_small001.ymap", ymap_bytes(&distant))]
}

/// Resource `smpl_one_side_light`: ships the LOD light pair with a light added to
/// `lodlights_small001.ymap` only.
fn one_side_light() -> ResourceFiles {
  let (mut lights, distant) = lod_lights("", 4);
  lights.lod_lights.push(lod_light(0x5A5A_1000));
  vec![
    ("stream/lodlights_small001.ymap", ymap_bytes(&lights)),
    ("stream/distlodlights_small001.ymap", ymap_bytes(&distant)),
  ]
}
