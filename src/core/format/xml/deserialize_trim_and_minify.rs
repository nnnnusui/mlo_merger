use regex::Regex;

use serde::{Deserialize, de};

pub fn deserialize_trim_and_minify<'de, D>(d: D) -> Result<String, D::Error>
where
  D: de::Deserializer<'de>,
{
  let de_string = String::deserialize(d)?;
  let re = Regex::new(r"\s+").unwrap();
  Ok(re.replace_all(de_string.trim(), " ").to_string())
}
