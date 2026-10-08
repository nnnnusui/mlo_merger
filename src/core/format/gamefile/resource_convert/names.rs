use super::*;

#[derive(serde::Deserialize)]
struct HashNameIndex {
  names: HashMap<String, String>,
}

pub(crate) fn load_vanilla_hash_names() -> HashMap<u32, String> {
  let path = Path::new("asset/vanilla/hash_names.json");
  match load_hash_names(path) {
    Ok(names) => names,
    Err(error) if error.kind() == io::ErrorKind::NotFound => HashMap::new(),
    Err(error) => {
      log::warn!("Failed to load vanilla hash names from {}: {error}", path.display());
      HashMap::new()
    }
  }
}

fn load_hash_names(path: &Path) -> io::Result<HashMap<u32, String>> {
  parse_hash_names(&fs::read(path)?)
}

fn parse_hash_names(bytes: &[u8]) -> io::Result<HashMap<u32, String>> {
  let index: HashNameIndex = serde_json::from_slice(bytes)
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
  Ok(
    index
      .names
      .into_iter()
      .filter_map(|(hash, name)| {
        let hash = u32::from_str_radix(&hash, 16).ok()?;
        (!name.eq_ignore_ascii_case(&format!("hash_{hash:08X}"))).then_some((hash, name))
      })
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hash_name_index_parses_hex_keys_and_ignores_fallback_names() {
    let bytes = br#"{"names":{"0000EF80":"glen2_ldoor_croc","0001D65D":"hash_0001D65D","invalid":"ignored"}}"#;
    let names = parse_hash_names(bytes).unwrap();
    assert_eq!(names.get(&0x0000_EF80).map(String::as_str), Some("glen2_ldoor_croc"));
    assert!(!names.contains_key(&0x0001_D65D));
  }
}
