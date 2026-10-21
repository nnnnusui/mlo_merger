use super::*;

impl MergeYmap {
  /// Writes each planned map: rebuilt maps as RSC7 binaries, single-source maps copied to
  /// `clone/`, and stale outputs of the rest removed.
  pub(super) fn write_outputs(
    &self,
    planned: BTreeMap<u32, PlannedMap>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let copy_targets_txt = self.output_dir.join("_copy_targets.txt");
    fs::create_dir_all(copy_targets_txt.parent().unwrap())?;
    let clone_ymap_dir = self.output_dir.join("clone");
    fs::create_dir_all(&clone_ymap_dir)?;
    let mut copy_targets_file = fs::File::create(copy_targets_txt)?;
    let managed =
      planned.values().map(|output| output.name.as_str()).collect::<Vec<_>>().join("\n");
    fs::write(self.output_dir.join("_managed_ymaps.txt"), managed)?;
    for output in planned.into_values() {
      let binary_path = self.output_dir.join(&output.name);
      let clone_path = clone_ymap_dir.join(&output.name);
      if output.rebuild {
        let entities = match &output.clone_entities {
          Some(entities) => runtime_entities(entities.iter()),
          None => runtime_entities(output.model.entities.iter()),
        }
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
        let bytes = crate::core::format::ymap::binary::write_ymap(&output.model, &entities)
          .map_err(|error| {
            std::io::Error::new(error.kind(), format!("Cannot write {}: {error}", output.name))
          })?;
        fs::write(&binary_path, bytes)?;
        if clone_path.exists() {
          fs::remove_file(&clone_path)?;
        }
        tracing::info!(
          "  [Success] Wrote merged/relinked YMAP binary to: {}",
          binary_path.display()
        );
      } else if let Some(target) = output.copy_target {
        use std::io::Write;
        writeln!(copy_targets_file, "{}", target.mod_ymap_path.display())?;
        fs::copy(&target.mod_ymap_path, &clone_path)?;
        tracing::info!(
          "Copied single-mod YMAP to clone output (vanilla diff merge skipped): {}",
          output.name
        );
      } else {
        if binary_path.exists() {
          fs::remove_file(&binary_path)?;
        }
        if clone_path.exists() {
          fs::remove_file(&clone_path)?;
        }
      }
    }

    Ok(())
  }
}
