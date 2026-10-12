//! Private downstream preparation-cache storage using shared JSON primitives.

use super::Result;
use crate::common::{json, path::safe_relative};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;

pub(crate) fn read_cache_entry<T: DeserializeOwned>(
  cache: &Path,
  name: &str,
) -> Result<Option<T>> {
  safe_relative(name)?;
  json::read_optional(&cache.join(name))
}

pub(crate) fn write_cache_entry(
  cache: &Path,
  name: &str,
  value: &impl Serialize,
) -> Result<()> {
  safe_relative(name)?;
  json::atomic_json(&cache.join(name), value)
}
