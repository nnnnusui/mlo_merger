//! The web demo's sample workspace under `web/sample`: an original vanilla archive and source
//! resources, built here so that no game data is shipped.
//!
//! Every name is invented (`smpl_*`, the DLCs `smpl_winter`, `smpl_patch_01` and `smpl_summer`,
//! and the game builds 1, 100 and 200), and everything sits around (-7100, 7100), in the sea off
//! the map. The vanilla archive ([`vanilla`]) has the stages `base`, `update`, `smpl_winter`
//! (build 100) with a title-update patch per build, `smpl_patch_01` and `smpl_summer` (build
//! 200), records what each DLC pack declares about the archives it mounts, and holds a
//! `materials.dat`.
//!
//! Each resource ([`resources`]) shows one behaviour of the merger a user meets on real data, and
//! is named after it; `web/sample/README.md` lists them as examples, by section, with what to
//! look at after a pipeline run:
//!
//! - Generations: the block's edits carried to its DLC generations, and the square, which
//!   `smpl_winter` replaces, omitted for its successor; a YMAP first shipped after build 100;
//!   changes left behind for each reason; YBN changes carried to the YBNs `smpl_summer` splits the
//!   yard into.
//! - Older versions of a vanilla file: resources made from the title update's park, from the
//!   collision before the kiosk and from the LOD lights before the title update's light, and one
//!   matching `smpl_winter`'s rink before its patches, which is not deployed.
//! - Game builds and DLC content: the title update's patches of `smpl_winter` per build, the
//!   archives the packs' change sets unload, and the stages each build ends at.
//! - Entity parents: indices repaired after a LOD entity is removed, LODs removed with all their
//!   children, a parent GUID two entities share, and a house left without its parent.
//! - Combining sources: the merge report's reasons, an added map two versions of a resource ship,
//!   and GUIDs rounded through a float.
//! - LOD light pairs, YBN repeats and surface names, and overwrites: one ignored overwrite of
//!   every reason, some written against earlier versions of the resources' files (see
//!   [`earlier_versions`]), the others in [`ignored::overwrites`], and [`collision_overwrite`],
//!   leaving the planter's collision of `smpl_street_tweaks` out.
//!
//! Every stream file covers [`EVERY_FILE_POSITION`].
//!
//! The tests check that `web/sample` holds what this module builds and each example's outcome,
//! one test each; run them with `MLO_WRITE_SAMPLES=1` to write the samples instead.

mod build;
mod earlier;
mod ignored;
mod resources;
#[cfg(test)]
mod tests;
mod vanilla;
mod ybn;

use earlier::earlier_versions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::core::format::gamefile::meta::jenk_hash;
use crate::core::format::ymap::binary::{write_ymap, ymap_to_model};
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::game_archive::{
  CacheVersion, CachedFile, FileChange, PatchVariant, VanillaCacheManifest,
};
use crate::platform::fs;
use sha2::{Digest, Sha256};
use ybn::Child;

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
  // The park's GUIDs lie far apart, so each rounds to a GUID of its own through a float.
  pub const OAK_1: u32 = 0x5310_2201;
  pub const OAK_2: u32 = 0x5320_4402;
  pub const OAK_3: u32 = 0x5330_6603;
  pub const FOUNTAIN: u32 = 0x5340_8804;
  pub const PARK_BENCH: u32 = 0x5350_AA05;
  pub const KIOSK: u32 = 0x5360_CC06;
  pub const STATUE_LOD: u32 = 0x534D_0300;
  pub const STATUE: u32 = 0x534D_0301;
  pub const SQUARE_BENCH: u32 = 0x534D_0302;
  pub const SQUARE_BIN: u32 = 0x534D_0303;
  pub const SQUARE_TREE: u32 = 0x534D_0304;
  pub const SQUARE_LAMP: u32 = 0x534D_0305;
  pub const SNOWMAN: u32 = 0x534D_0306;
  pub const PIER_BOLLARD_1: u32 = 0x534D_0401;
  pub const PIER_BOLLARD_2: u32 = 0x534D_0402;
  pub const RINK_BARRIER: u32 = 0x534D_0501;
  pub const RINK_LAMP: u32 = 0x534D_0502;
  pub const RINK_BENCH: u32 = 0x534D_0503;
  pub const HUT: u32 = 0x534D_0504;
  pub const ROW_SLOD: u32 = 0x534D_0600;
  pub const HOUSE_C_LOD: u32 = 0x534D_0601;
  pub const HOUSE_D_LOD: u32 = 0x534D_0602;
  pub const HOUSE_E_LOD: u32 = 0x534D_0603;
  pub const HOUSE_F_LOD: u32 = 0x534D_0604;
  pub const HOUSE_C: u32 = 0x534D_0611;
  pub const HOUSE_D: u32 = 0x534D_0612;
  pub const HOUSE_E: u32 = 0x534D_0613;
  pub const HOUSE_F: u32 = 0x534D_0614;
  pub const CRANE_LOD: u32 = 0x534D_0700;
  pub const CRANE: u32 = 0x534D_0701;
  pub const QUAY_BOLLARD: u32 = 0x534D_0702;
  pub const BLOCK_99_BENCH: u32 = 0x534D_0901;
  pub const PLANTER: u32 = 0x5A5A_0001;
  pub const BIRCH: u32 = 0x5A5A_0002;
  pub const CONE: u32 = 0x5A5A_0003;
  pub const SIGN: u32 = 0x5A5A_0101;
  pub const CRATE: u32 = 0x5A5A_0102;
  pub const CRATE_LABEL: u32 = 0x5A5A_0103;
  pub const CRATE_V2: u32 = 0x5A5A_0202;
  pub const BARREL: u32 = 0x5A5A_0203;
  pub const PARK_BIN: u32 = 0x5A5A_0301;
  pub const OLD_SIGN: u32 = 0x5A5A_0401;
  pub const SQUARE_SIGN: u32 = 0x5A5A_0501;
  pub const FLOWER_BED: u32 = 0x5A5A_0502;
}

/// A point every stream file of the sample covers: inside each YMAP's entity extents and each
/// YBN's bounds, so a file-from-position search there lists them all.
pub const EVERY_FILE_POSITION: [f32; 3] = [-7090.0, 7092.0, 20.5];

/// One file of the sample workspace, by its path below the workspace.
struct SampleFile {
  path: String,
  bytes: Vec<u8>,
}

/// Which entity of `map` has `guid`.
fn find(
  map: &Ymap,
  guid: u32,
) -> usize {
  map.entities.iter().position(|entity| entity.guid == guid).unwrap()
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
  "smpl_square",
  "smpl_square_lod",
  "winter_smpl_square",
  "smpl_pier",
  "winter_smpl_rink",
  "winter_smpl_hut",
  "smpl_row",
  "smpl_row_lod",
  "smpl_quay",
  "smpl_quay_lod",
  "smpl_custom_01",
  "smpl_custom_02",
  "smpl_custom_old",
  "smpl_block_99",
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
  "smpl_cone",
  "smpl_tree_birch",
  "smpl_sign_board",
  "smpl_crate",
  "smpl_crate_label",
  "smpl_barrel",
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
  "smpl_statue",
  "smpl_statue_lod",
  "smpl_snowman",
  "smpl_bollard",
  "smpl_rink_barrier",
  "smpl_hut",
  "smpl_house_c",
  "smpl_house_d",
  "smpl_house_e",
  "smpl_house_f",
  "smpl_house_c_lod",
  "smpl_house_d_lod",
  "smpl_house_e_lod",
  "smpl_house_f_lod",
  "smpl_row_slod",
  "smpl_crane",
  "smpl_crane_lod",
  "smpl_flower_bed",
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

/// The overwrite of `smpl_street_tweaks`'s `smpl_block_01.ybn` leaving out the box it adds, the
/// planter's collision, written against the collision `smpl_summer` ships.
fn collision_overwrite() -> (String, Vec<u8>) {
  let fingerprint =
    |bytes: &[u8]| serde_json::json!({"sha256": sha256(bytes), "size": bytes.len()});
  let source = resources::street_tweaks(false)
    .into_iter()
    .find(|(path, _)| path.ends_with(".ybn"))
    .map(|(_, bytes)| bytes)
    .unwrap();
  let overwrite = serde_json::json!({
    "schema_version": crate::app::map::overwrite::StreamFileOverwrite::SCHEMA_VERSION,
    "target": fingerprint(&source),
    "base": fingerprint(&ybn_bytes(&vanilla::block_01_collision(true))),
    "skip": {"polygons": [{"flags": [0, 0], "added": [{"shape": {"type": "box"}}]}]},
  });
  (
    "asset/map/overwrite/resources/smpl_street_tweaks/smpl_block_01.ybn.diff.json".into(),
    serde_json::to_vec_pretty(&overwrite).unwrap(),
  )
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

[ybn_repeats]
enabled = true
vanilla_min_share = 0.05
";

/// Every file of the sample workspace, with `files.json` listing them for the page.
fn sample_files() -> Vec<SampleFile> {
  let mut files = workspace_files(false);
  let overwrites = ignored::overwrites().into_iter().chain([collision_overwrite()]);
  files.extend(earlier_versions().iter().cloned().chain(overwrites).map(|(path, bytes)| {
    SampleFile {
      path: path.clone(),
      bytes: bytes.clone(),
    }
  }));
  // The files the page seeds.
  let mut listed = files.iter().map(|file| file.path.clone()).collect::<Vec<_>>();
  listed.sort();
  files.push(SampleFile {
    path: "files.json".into(),
    bytes: serde_json::to_vec_pretty(&listed).unwrap(),
  });
  files
}

/// The vanilla archive, the configuration and the resources, at their `earlier` versions or
/// their current ones.
fn workspace_files(earlier: bool) -> Vec<SampleFile> {
  let mut files = Vec::new();
  let (manifest, objects) = vanilla_manifest();
  files.extend(objects.into_iter().map(|(object, bytes)| SampleFile {
    path: format!("asset/vanilla/{object}"),
    bytes,
  }));
  files.push(SampleFile {
    path: "asset/vanilla/cache_info.json".into(),
    bytes: serde_json::to_vec_pretty(&manifest).unwrap(),
  });
  files.push(SampleFile {
    path: format!("asset/vanilla/{}", crate::core::game_archive::MATERIALS_DAT),
    bytes: vanilla::materials_dat().into_bytes(),
  });
  let names = hash_names()
    .into_iter()
    .map(|(hash, name)| (format!("{hash:08X}"), name))
    .collect::<BTreeMap<_, _>>();
  files.push(SampleFile {
    path: "asset/vanilla/hash_names.json".into(),
    bytes: serde_json::to_vec_pretty(&serde_json::json!({"names": names})).unwrap(),
  });
  for (resource, resource_files) in resources::resources(earlier) {
    files.push(SampleFile {
      path: format!("asset/map/source/{resource}/fxmanifest.lua"),
      bytes: FXMANIFEST.into(),
    });
    for (path, bytes) in resource_files {
      files.push(SampleFile {
        path: format!("asset/map/source/{resource}/{path}"),
        bytes,
      });
    }
  }
  files.push(SampleFile {
    path: "asset/config.toml".into(),
    bytes: CONFIG.into(),
  });
  files
}

/// The vanilla archive's manifest of [`vanilla::stages`], and its files by their object path, as
/// `--generate-vanilla` would store them: each stage's under `{stage}/{extension}/`, and each
/// version of a pack's title-update patch under `{stage}/update_rpf/{name}/{extension}/`, its
/// changes relative to the files the installed game loads before it.
fn vanilla_manifest() -> (VanillaCacheManifest, Vec<(String, Vec<u8>)>) {
  let mut objects = Vec::new();
  // The latest version of each file as the installed game loads the stages so far.
  let mut installed = BTreeMap::<String, String>::new();
  let mut changes_of =
    |directory: &str, files: Vec<vanilla::StageFile>, installed: &BTreeMap<String, String>| {
      let mut changes = BTreeMap::new();
      for (name, archive, bytes) in files {
        let extension = name.rsplit_once('.').unwrap().1;
        let object = format!("{directory}/{extension}/{name}");
        changes.insert(
          name.to_owned(),
          FileChange {
            previous_sha256: installed.get(name).cloned(),
            file: CachedFile {
              sha256: sha256(&bytes),
              object: object.clone(),
              source: format!("{archive}/{name}"),
            },
          },
        );
        objects.push((object, bytes));
      }
      changes
    };
  let mut versions = Vec::<CacheVersion>::new();
  for (index, stage) in vanilla::stages().into_iter().enumerate() {
    let id = format!("{index:04}-{}", stage.label);
    let changes = changes_of(&id, stage.files, &installed);
    for (name, change) in &changes {
      installed.insert(name.clone(), change.file.sha256.clone());
    }
    let mut patches = Vec::new();
    for patch in stage.patches {
      let patch_id = format!("{id}/update_rpf/{}", patch.name);
      let patch_changes = changes_of(&patch_id, patch.files, &installed);
      patches.push((
        patch.installed,
        PatchVariant {
          id: patch_id,
          builds: patch.builds,
          installed: patch.installed,
          archives: vec![format!("update/update.rpf/dlc_patch/{}", stage.label)],
          changes: patch_changes,
          file_kinds: BTreeMap::new(),
          unchanged: 0,
          dlc: stage.dlc.clone(),
        },
      ));
    }
    for (_, patch) in patches.iter().filter(|(installed, _)| *installed) {
      for (name, change) in &patch.changes {
        installed.insert(name.clone(), change.file.sha256.clone());
      }
    }
    versions.push(CacheVersion {
      id,
      parent: versions.last().map(|previous| previous.id.clone()),
      archives: stage.archives,
      changes,
      file_kinds: BTreeMap::new(),
      unchanged: 0,
      dlc: stage.dlc,
      patches: patches.into_iter().map(|(_, patch)| patch).collect(),
    });
  }
  let manifest = VanillaCacheManifest {
    format_version: crate::core::game_archive::FORMAT_VERSION,
    contents: [crate::core::game_archive::ArchiveContent::Map].into(),
    game_dir: "/mnt/gtav".into(),
    title_updates: vanilla::TITLE_UPDATES
      .iter()
      .map(|(build, title_update)| (*build, (*title_update).to_owned()))
      .collect(),
    versions,
  };
  (manifest, objects)
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
