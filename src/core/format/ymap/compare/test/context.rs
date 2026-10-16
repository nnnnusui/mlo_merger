//! Loading, caching and parent resolution of [`YmapComparisonContext`] over the local caches.

use std::{collections::HashMap, fs, path::Path, rc::Rc};

use crate::core::format::ymap::{
  compare::{StreamFileRef, YmapComparisonContext, YmapParentRef},
  diff::reference_hash,
};

#[test]
#[ignore = "requires local asset/source-cache and asset/vanilla-cache"]
fn reads_vanilla_and_source_ymaps_once() {
  let asset = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset");
  let mut context = YmapComparisonContext::new(
    &asset.join("source-cache"),
    &asset.join("vanilla-cache"),
    HashMap::new(),
  )
  .unwrap();
  // Pick any uniquely named source YMAP that overrides a vanilla one.
  let latest = asset.join("vanilla-cache/latest/ymap");
  let (resource_name, file_name) = context
    .source_index
    .paths
    .iter()
    .find(|((_, file_name), paths)| paths.len() == 1 && latest.join(file_name).is_file())
    .map(|(key, _)| key.clone())
    .expect("no source YMAP overrides a vanilla one");
  let vanilla = StreamFileRef {
    resource_name: None,
    vanilla_version: context.vanilla_version.clone(),
    file_name,
  };
  let source = StreamFileRef {
    resource_name: Some(resource_name),
    ..vanilla.clone()
  };

  let first = context.get_cache(&vanilla).unwrap();
  assert!(Rc::ptr_eq(&context.get_cache(&vanilla).unwrap(), &first));
  let entity_count = context.get_cache(&source).unwrap().entities.len();
  let data = context.get_prepared(&source).unwrap();
  assert_eq!(data.entity_indices_by_guid.values().map(Vec::len).sum::<usize>(), entity_count);
  assert_eq!(context.prepared_by_ref.len(), 1);
  // The diff also prepares the vanilla baseline.
  context.get_diff(&source).unwrap();
  assert_eq!(context.cache_by_ref.len(), 2);
  assert_eq!(context.prepared_by_ref.len(), 2);

  // A resource without the requested YMAP falls back to vanilla.
  let missing_in_resource = fs::read_dir(&latest)
    .unwrap()
    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
    .find(|file_name| {
      !context
        .source_index
        .paths
        .contains_key(&(source.resource_name.clone().unwrap(), file_name.clone()))
    })
    .unwrap();
  let fallback = StreamFileRef {
    file_name: missing_in_resource.clone(),
    ..source.clone()
  };
  let fallback_ymap = context.get_cache(&fallback).unwrap();
  let vanilla_ymap = context
    .get_cache(&StreamFileRef {
      resource_name: None,
      ..fallback.clone()
    })
    .unwrap();
  assert_eq!(fallback_ymap, vanilla_ymap);

  let stale = StreamFileRef {
    vanilla_version: "0000-base".into(),
    ..vanilla
  };
  assert!(context.get_cache(&stale).is_err());
}

#[test]
#[ignore = "requires local asset/source-cache and asset/vanilla-cache"]
fn resolves_parents_of_every_source_ymap() {
  let asset = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset");
  let mut context = YmapComparisonContext::new(
    &asset.join("source-cache"),
    &asset.join("vanilla-cache"),
    HashMap::new(),
  )
  .unwrap();
  let files = context
    .source_index
    .paths
    .iter()
    .filter(|(_, paths)| paths.len() == 1)
    .map(|((resource_name, file_name), _)| StreamFileRef {
      resource_name: Some(resource_name.clone()),
      vanilla_version: context.vanilla_version.clone(),
      file_name: file_name.clone(),
    })
    .collect::<Vec<_>>();
  let (mut with_parent, mut source_parents, mut unresolved) = (0, 0, 0);
  let mut failures = Vec::new();
  for file in &files {
    let parent = match context.get_prepared(file) {
      Ok(data) => data.parent_ref.clone(),
      Err(error) => {
        failures.push(format!("{file:?}: {error}"));
        continue;
      }
    };
    let parent = match parent {
      None => continue,
      Some(YmapParentRef::Unresolved(_)) => {
        unresolved += 1;
        continue;
      }
      Some(YmapParentRef::Resolved(parent)) => parent,
    };
    with_parent += 1;
    source_parents += usize::from(parent.resource_name.is_some());
    if let Err(error) = context.get_cache(&parent) {
      failures.push(format!("{file:?} -> {parent:?}: {error}"));
    }
  }
  println!(
    "{} source YMAPs, {with_parent} with a resolved parent ({source_parents} in the same resource), {unresolved} unresolved",
    files.len()
  );
  assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[derive(serde::Serialize)]
struct ParentSide {
  /// Declared parent as `hash_XXXXXXXX`, or `None` without a parent.
  parent_hash: Option<String>,
  parent_ref: Option<YmapParentRef>,
}

#[derive(serde::Serialize)]
struct ParentChange {
  resource_name: String,
  file_name: String,
  vanilla: ParentSide,
  source: ParentSide,
}

#[derive(serde::Serialize)]
struct ParentChangeReport {
  /// Source YMAPs that override a vanilla YMAP of the same name.
  compared: usize,
  changed: usize,
  changes: Vec<ParentChange>,
}

#[test]
#[ignore = "requires local asset/source-cache and asset/vanilla-cache"]
fn dump_source_ymaps_with_changed_parents() {
  let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
  let asset = manifest.join("asset");
  let mut context = YmapComparisonContext::new(
    &asset.join("source-cache"),
    &asset.join("vanilla-cache"),
    HashMap::new(),
  )
  .unwrap();
  let files = context
    .source_index
    .paths
    .iter()
    .filter(|((_, file_name), paths)| {
      paths.len() == 1 && context.vanilla_file_names.contains(file_name)
    })
    .map(|((resource_name, file_name), _)| (resource_name.clone(), file_name.clone()))
    .collect::<Vec<_>>();
  let mut changes = Vec::new();
  for (resource_name, file_name) in &files {
    let vanilla = StreamFileRef {
      resource_name: None,
      vanilla_version: context.vanilla_version.clone(),
      file_name: file_name.clone(),
    };
    let source = StreamFileRef {
      resource_name: Some(resource_name.clone()),
      ..vanilla.clone()
    };
    let mut side = |file: &StreamFileRef| {
      let hash = reference_hash(&context.get_cache(file).unwrap().parent);
      ParentSide {
        parent_hash: (hash != 0).then(|| format!("hash_{hash:08X}")),
        parent_ref: context.get_prepared(file).unwrap().parent_ref.clone(),
      }
    };
    let (vanilla, source) = (side(&vanilla), side(&source));
    if vanilla.parent_hash != source.parent_hash {
      changes.push(ParentChange {
        resource_name: resource_name.clone(),
        file_name: file_name.clone(),
        vanilla,
        source,
      });
    }
  }
  let report = ParentChangeReport {
    compared: files.len(),
    changed: changes.len(),
    changes,
  };
  let output = manifest.join("target/ymap_compare/parent_changes.json");
  fs::create_dir_all(output.parent().unwrap()).unwrap();
  fs::write(&output, serde_json::to_string_pretty(&report).unwrap()).unwrap();
  println!("{} of {} parents changed -> {}", report.changed, report.compared, output.display());
}
