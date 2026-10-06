//! Strict raw-file loading and hash validation for vanilla variants.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use super::{
  ModelReader, Result,
  comparison::ModelState,
  io::{cache_path, content_hash},
};
use crate::core::{
  format::ybn::{model::Bound, read_ybn},
  vanilla::CachedFile,
};

pub(super) struct NativeVariants<'a> {
  pub(super) cache: &'a Path,
  pub(super) reader: ModelReader,
  pub(super) models: BTreeMap<String, Rc<ModelState>>,
  pub(super) ybn_models: BTreeMap<String, Rc<Bound>>,
}

impl NativeVariants<'_> {
  pub(super) fn model(
    &mut self,
    file: &CachedFile,
  ) -> Result<Rc<ModelState>> {
    if let Some(model) = self.models.get(&file.sha256) {
      return Ok(model.clone());
    }
    let path = self.raw_path(file, ".ymap")?;
    let model = Rc::new(ModelState::new((self.reader)(&path)?)?);
    self.models.insert(file.sha256.clone(), model.clone());
    Ok(model)
  }

  pub(super) fn ybn_model(
    &mut self,
    file: &CachedFile,
  ) -> Result<Rc<Bound>> {
    if let Some(model) = self.ybn_models.get(&file.sha256) {
      return Ok(model.clone());
    }
    let path = self.raw_path(file, ".ybn")?;
    let model = Rc::new(read_ybn(&fs::read(path)?)?);
    self.ybn_models.insert(file.sha256.clone(), model.clone());
    Ok(model)
  }

  fn raw_path(
    &self,
    file: &CachedFile,
    extension: &str,
  ) -> Result<std::path::PathBuf> {
    if !file.object.ends_with(extension) {
      return Err(
        format!("Vanilla artifact for {} is not a raw {extension} file", file.source).into(),
      );
    }
    if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
      return Err("Invalid native SHA-256 in vanilla cache".into());
    }
    let path = cache_path(self.cache, &file.object)?;
    if !path.is_file() {
      return Err(format!("Raw vanilla artifact is missing: {}", path.display()).into());
    }
    if content_hash(&path)? != file.sha256 {
      return Err(format!("Native snapshot hash mismatch: {}", path.display()).into());
    }
    Ok(path)
  }
}
