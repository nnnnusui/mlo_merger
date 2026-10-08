use super::*;

impl MergeYmap {
  pub(super) fn write_outputs(
    &self,
    planned: BTreeMap<u32, PlannedMap>,
    binary_catalog: Option<&crate::core::format::gamefile::meta_resource::MetaSchemaCatalog>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let copy_targets_txt = self.output_dir.join("_copy_targets.txt");
    fs::create_dir_all(copy_targets_txt.parent().unwrap())?;
    let clone_ymap_dir = self.output_dir.join("clone");
    fs::create_dir_all(&clone_ymap_dir)?;
    let mut copy_targets_file = fs::File::create(copy_targets_txt)?;
    let managed = planned
      .values()
      .map(|output| output.name.trim_end_matches(".xml"))
      .collect::<Vec<_>>()
      .join("\n");
    fs::write(self.output_dir.join("_managed_ymaps.txt"), managed)?;
    for output in planned.into_values() {
      let xml_path = self.output_dir.join(&output.name);
      let binary_name = output.name.trim_end_matches(".xml");
      let binary_path = self.output_dir.join(binary_name);
      let clone_path = clone_ymap_dir.join(binary_name);
      if output.rebuild {
        if let Some(catalog) = binary_catalog {
          let entities = match &output.clone_entities {
            Some(entities) => runtime_entities(entities.iter()),
            None => runtime_entities(output.model.entity_map.values()),
          }
          .into_iter()
          .cloned()
          .collect::<Vec<_>>();
          let bytes =
            crate::core::format::ymap::binary::write_ymap(&output.model, &entities, catalog)
              .map_err(|error| {
                std::io::Error::new(error.kind(), format!("Cannot write {binary_name}: {error}"))
              })?;
          fs::write(&binary_path, bytes)?;
          if xml_path != binary_path && xml_path.exists() {
            fs::remove_file(&xml_path)?;
          }
          if clone_path.exists() {
            fs::remove_file(&clone_path)?;
          }
          log::info!("  [Success] Wrote merged/relinked YMAP binary to: {}", binary_path.display());
          continue;
        }
        let xml_string = if let Some(entities) = &output.clone_entities {
          if output.original.xml.is_empty() {
            let mut model = output.model.clone();
            model.entity_map =
              entities.iter().map(|entity| (entity.guid, entity.clone())).collect();
            let xml_ymap: XmlYmap = model.into();
            let mut xml = String::new();
            let mut serializer = quick_xml::se::Serializer::new(&mut xml);
            serializer.indent(' ', 2);
            xml_ymap.serialize(serializer)?;
            xml
          } else {
            patch_clone(&output.original, entities)?
          }
        } else {
          let xml_ymap: XmlYmap = output.model.into();
          let mut xml = String::new();
          let mut serializer = quick_xml::se::Serializer::new(&mut xml);
          serializer.indent(' ', 2);
          xml_ymap.serialize(serializer)?;
          xml
        };
        fs::write(&xml_path, xml_string)?;
        if clone_path.exists() {
          fs::remove_file(&clone_path)?;
        }
        log::info!("  [Success] Wrote merged/relinked YMAP to: {}", xml_path.display());
      } else if let Some(target) = output.copy_target {
        let ymap_xml_name = target.mod_ymap_path.file_name().unwrap().to_string_lossy();
        let extracted_ymap_name = ymap_xml_name.trim_end_matches(".xml");
        use std::io::Write;
        writeln!(copy_targets_file, "{}", ymap_xml_name)?;
        fs::copy(self.mod_ymap_dir.join(extracted_ymap_name), &clone_path)?;
        if xml_path.exists() {
          fs::remove_file(&xml_path)?;
        }
        log::info!(
          "Copied single-mod YMAP to clone output (vanilla diff merge skipped): {binary_name}"
        );
      } else {
        if binary_catalog.is_some() && binary_path.exists() {
          fs::remove_file(&binary_path)?;
        }
        if xml_path.exists() {
          fs::remove_file(&xml_path)?;
        }
        if clone_path.exists() {
          fs::remove_file(&clone_path)?;
        }
      }
    }

    Ok(())
  }
}
