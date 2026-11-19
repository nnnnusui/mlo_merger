//! A collection's `CPedVariationInfo`, kept as the XML tree every container converts to, so
//! that merging can renumber the references it knows and carry every other field over.

use std::{collections::HashMap, io};

use crate::core::format::gamefile::{
  meta::MetaResource,
  resource::Resource,
  xml_tree::{XmlElement, parse_xml},
};

use super::names::{COMPONENT_NAMES, Slot};

/// The root of a Ped variation YMT.
pub const ROOT: &str = "CPedVariationInfo";

/// Prop anchors as file names spell them after `p_`, with their `eAnchorPoints` names.
pub const ANCHORS: [(&str, &str); 11] = [
  ("head", "ANCHOR_HEAD"),
  ("eyes", "ANCHOR_EYES"),
  ("ears", "ANCHOR_EARS"),
  ("mouth", "ANCHOR_MOUTH"),
  ("lhand", "ANCHOR_LEFT_HAND"),
  ("rhand", "ANCHOR_RIGHT_HAND"),
  ("lwrist", "ANCHOR_LEFT_WRIST"),
  ("rwrist", "ANCHOR_RIGHT_WRIST"),
  ("hip", "ANCHOR_HIP"),
  ("lfoot", "ANCHOR_LEFT_FOOT"),
  ("rfoot", "ANCHOR_RIGHT_FOOT"),
];

/// Reads a `CPedVariationInfo` YMT from RSC7 META, PSO or XML.
pub(crate) fn read_variation(bytes: &[u8]) -> io::Result<XmlElement> {
  let xml = match bytes.get(..4) {
    Some(b"RSC7" | b"PSIN") => match Resource::decode(bytes)? {
      Resource::Rsc7(resource) => MetaResource::parse(&resource)?.to_xml(&HashMap::new())?,
      Resource::Pso(resource) => resource.to_xml(&HashMap::new())?,
    },
    Some(b"RBF0") => return Err(invalid("RBF clothing YMTs are not supported")),
    _ => String::from_utf8(bytes.to_vec())
      .map_err(|_| invalid("Unsupported binary clothing YMT format"))?,
  };
  let root = parse_xml(xml.trim_start_matches('\u{feff}'))?;
  if root.name != ROOT {
    return Err(invalid(&format!("YMT root is {}, not {ROOT}", root.name)));
  }
  Ok(root)
}

/// One drawable of a component, as the YMT declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredDrawable {
  /// Its texture count.
  pub textures: usize,
  /// `numAlternatives`.
  pub alternatives: u32,
  /// Whether it owns cloth (`clothData/ownsCloth`).
  pub cloth: bool,
}

/// What a `CPedVariationInfo` declares, per component and prop anchor, to match against the
/// files a collection streams.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
  /// Each slot's drawables, or its props' texture counts as [`DeclaredDrawable::textures`].
  pub slots: Vec<(Slot, Vec<DeclaredDrawable>)>,
}

impl Declared {
  /// Reads the declared drawables and props of a `CPedVariationInfo` root.
  pub(crate) fn of(root: &XmlElement) -> io::Result<Self> {
    let mut slots = Vec::new();
    let available = child(root, "availComp")
      .map(|available| {
        available
          .text
          .split_whitespace()
          .map(|value| value.parse::<u8>().map_err(|_| invalid("Invalid availComp value")))
          .collect::<io::Result<Vec<_>>>()
      })
      .transpose()?
      .unwrap_or_default();
    let components = items(root, "aComponentData3");
    for (slot, &index) in available.iter().enumerate().take(COMPONENT_NAMES.len()) {
      if index == u8::MAX {
        continue;
      }
      let component = components
        .get(usize::from(index))
        .ok_or_else(|| invalid("availComp names a missing component"))?;
      let drawables = items(component, "aDrawblData3")
        .into_iter()
        .map(|drawable| {
          Ok(DeclaredDrawable {
            textures: items(drawable, "aTexData").len(),
            alternatives: value(drawable, "numAlternatives")?.unwrap_or(0),
            cloth: child(drawable, "clothData")
              .and_then(|cloth| child(cloth, "ownsCloth"))
              .and_then(|owns| owns.attributes.get("value"))
              .is_some_and(|owns| owns == "true"),
          })
        })
        .collect::<io::Result<Vec<_>>>()?;
      slots.push((Slot::Component(slot), drawables));
    }
    if let Some(info) = child(root, "propInfo") {
      for anchor in items(info, "aAnchors") {
        let name = child(anchor, "anchor").map_or("", |anchor| anchor.text.as_str());
        let file_name = ANCHORS
          .iter()
          .find(|(_, anchor)| *anchor == name)
          .map(|(file_name, _)| (*file_name).to_owned())
          .ok_or_else(|| invalid(&format!("Unknown prop anchor {name}")))?;
        let props = child(anchor, "props")
          .map(|props| {
            props
              .text
              .split_whitespace()
              .map(|textures| {
                Ok(DeclaredDrawable {
                  textures: textures.parse().map_err(|_| invalid("Invalid prop texture count"))?,
                  alternatives: 0,
                  cloth: false,
                })
              })
              .collect::<io::Result<Vec<_>>>()
          })
          .transpose()?
          .unwrap_or_default();
        slots.push((Slot::Prop(file_name), props));
      }
    }
    Ok(Self {
      slots,
    })
  }
}

fn child<'a>(
  element: &'a XmlElement,
  name: &str,
) -> Option<&'a XmlElement> {
  element.children.iter().find(|child| child.name == name)
}

/// The `Item`s of the array `name` of `element`; none when it is missing.
fn items<'a>(
  element: &'a XmlElement,
  name: &str,
) -> Vec<&'a XmlElement> {
  child(element, name).map_or_else(Vec::new, |array| array.children.iter().collect())
}

fn value(
  element: &XmlElement,
  name: &str,
) -> io::Result<Option<u32>> {
  child(element, name)
    .and_then(|child| child.attributes.get("value"))
    .map(|value| value.parse().map_err(|_| invalid(&format!("Invalid {name} value"))))
    .transpose()
}

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
  use super::*;

  const VARIATION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<CPedVariationInfo name="mp_m_smpl_pack">
 <availComp>255 255 255 255 255 255 255 255 255 255 255 0</availComp>
 <aComponentData3 itemType="CPVComponentData">
  <Item>
   <numAvailTex value="3" />
   <aDrawblData3 itemType="CPVDrawblData">
    <Item>
     <propMask value="1" />
     <numAlternatives value="0" />
     <aTexData itemType="CPVTextureData"><Item /><Item /></aTexData>
     <clothData><ownsCloth value="false" /></clothData>
    </Item>
    <Item>
     <propMask value="1" />
     <numAlternatives value="1" />
     <aTexData itemType="CPVTextureData"><Item /></aTexData>
     <clothData><ownsCloth value="true" /></clothData>
    </Item>
   </aDrawblData3>
  </Item>
 </aComponentData3>
 <propInfo>
  <numAvailProps value="2" />
  <aAnchors itemType="CAnchorProps">
   <Item><props>3 1</props><anchor>ANCHOR_LEFT_WRIST</anchor></Item>
  </aAnchors>
 </propInfo>
</CPedVariationInfo>"#;

  #[test]
  fn declared_drawables_and_props_follow_avail_comp_and_anchors() {
    let root = read_variation(VARIATION.as_bytes()).unwrap();
    let declared = Declared::of(&root).unwrap();
    let drawable = |textures, alternatives, cloth| DeclaredDrawable {
      textures,
      alternatives,
      cloth,
    };
    assert_eq!(
      declared.slots,
      [
        (Slot::Component(11), vec![drawable(2, 0, false), drawable(1, 1, true)]),
        (Slot::Prop("lwrist".into()), vec![drawable(3, 0, false), drawable(1, 0, false)]),
      ]
    );
  }

  #[test]
  fn other_roots_and_rbf_are_rejected() {
    assert!(read_variation(b"<CCreatureMetaData />").is_err());
    assert!(read_variation(b"RBF0\0\0").is_err());
    assert!(
      Declared::of(
        &read_variation(b"<CPedVariationInfo><availComp>0</availComp></CPedVariationInfo>")
          .unwrap()
      )
      .is_err()
    );
  }

  /// Every local Ped variation YMT reads, and declares only known slots.
  #[test]
  #[ignore = "requires local asset/clothing/source"]
  fn local_variations_read() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/clothing/source");
    let mut stack = vec![root];
    let mut read = 0;
    while let Some(directory) = stack.pop() {
      for entry in crate::platform::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
          stack.push(path);
          continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
        if !name.ends_with(".ymt") || name.contains("creaturemetadata") {
          continue;
        }
        let bytes = crate::platform::fs::read(&path).unwrap();
        let root = read_variation(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        Declared::of(&root).unwrap_or_else(|error| panic!("{name}: {error}"));
        read += 1;
      }
    }
    assert!(read > 0);
  }
}
