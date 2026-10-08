use super::*;

pub(super) struct Group {
  pub(super) extension: &'static str,
  pub(super) names: BTreeSet<String>,
}

#[derive(Deserialize)]
struct Relationships {
  children_by_parent_hash: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct SourceParents {
  resources: BTreeMap<String, ParentResource>,
}

#[derive(Deserialize)]
struct ParentResource {
  files_by_format: BTreeMap<String, Vec<ParentFile>>,
}

#[derive(Deserialize)]
struct ParentFile {
  file_name: String,
  ymap_parent_hash: Option<String>,
}

impl IncrementalMerge<'_> {
  pub(super) fn ymap_graph(&self) -> Result<BTreeMap<String, BTreeSet<String>>> {
    if self.inputs.ymap.is_empty() {
      return Ok(BTreeMap::new());
    }
    let mut hashes = BTreeMap::new();
    let mut graph = BTreeMap::new();
    for path in self.ymap {
      let name = file_name(path)?;
      let hash = reference_hash(name.trim_end_matches(".ymap"));
      if hashes.insert(hash, name.clone()).is_some() {
        return Err("Ambiguous vanilla YMAP hash in merge dependencies".into());
      }
      graph.insert(name, BTreeSet::new());
    }
    let relationships: Relationships = serde_json::from_reader(BufReader::new(fs::File::open(
      self.vanilla_cache.join("ymap_relationships.json"),
    )?))?;
    for (parent_hash, children) in relationships.children_by_parent_hash {
      if let Ok(hash) = u32::from_str_radix(&parent_hash, 16)
        && let Some(parent) = hashes.get(&hash)
      {
        for child in children {
          connect(&mut graph, parent, &child.to_ascii_lowercase());
        }
      }
    }
    let metadata: SourceParents = serde_json::from_reader(BufReader::new(fs::File::open(
      self.source_cache.join("source_cache_info.json"),
    )?))?;
    for resource in metadata.resources.values() {
      for source in resource.files_by_format.get(".ymap").into_iter().flatten() {
        if let Some(hash) = &source.ymap_parent_hash
          && let Ok(hash) = u32::from_str_radix(hash, 16)
          && let Some(parent) = hashes.get(&hash)
        {
          connect(&mut graph, parent, &source.file_name.to_ascii_lowercase());
        }
      }
    }
    Ok(graph)
  }
}

fn connect(
  graph: &mut BTreeMap<String, BTreeSet<String>>,
  parent: &str,
  child: &str,
) {
  if parent == child || !graph.contains_key(parent) || !graph.contains_key(child) {
    return;
  }
  graph.get_mut(parent).unwrap().insert(child.into());
  graph.get_mut(child).unwrap().insert(parent.into());
}

pub(super) fn groups(
  sources: &BTreeMap<String, Vec<&MergeSourceFile>>,
  graph: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<Group> {
  let mut visited = BTreeSet::new();
  let mut groups = Vec::new();
  for name in sources.keys() {
    if !visited.insert(name.clone()) {
      continue;
    }
    let extension = if name.ends_with(".ybn") { "ybn" } else { "ymap" };
    let mut names = BTreeSet::from([name.clone()]);
    let mut pending = vec![name.clone()];
    while let Some(current) = pending.pop() {
      for neighbor in graph.get(&current).into_iter().flatten() {
        if visited.insert(neighbor.clone()) {
          names.insert(neighbor.clone());
          pending.push(neighbor.clone());
        }
      }
    }
    groups.push(Group {
      extension,
      names,
    });
  }
  groups
}
