use super::*;

/// Patches only parent-related XML fields in a promoted clone, preserving all other payloads.
pub(in crate::core::merge) fn patch_clone(
  original: &OriginalMap,
  entities: &[YmapEntity],
) -> io::Result<String> {
  use quick_xml::{
    Reader, Writer,
    events::{BytesStart, BytesText, Event},
  };
  let mut reader = Reader::from_str(&original.xml);
  let mut writer = Writer::new(Vec::new());
  let mut stack = Vec::<String>::new();
  let mut count = 0usize;
  let mut current = None;
  loop {
    let event = reader.read_event().map_err(|error| invalid(&error.to_string()))?;
    let empty = matches!(&event, Event::Empty(_));
    match event {
      Event::Start(start) | Event::Empty(start) => {
        let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
        if name == "Item" && stack.last().is_some_and(|parent| parent == "entities") {
          current = Some(count);
          count += 1;
        }
        let direct = stack.last().is_some_and(|parent| parent == "Item")
          && stack.get(stack.len().saturating_sub(2)).is_some_and(|parent| parent == "entities");
        let replacement = if direct {
          let entity = entities
            .get(current.ok_or_else(|| invalid("clone entity context missing"))?)
            .ok_or_else(|| invalid("clone entity count differs"))?;
          match name.as_str() {
            "parentIndex" => Some(entity.parent_index.to_string()),
            "flags" => Some(entity.flags.to_string()),
            "numChildren" => Some(entity.num_children.to_string()),
            _ => None,
          }
        } else {
          None
        };
        let element = if let Some(value) = replacement {
          let mut element = BytesStart::new(name.clone());
          element.push_attribute(("value", value.as_str()));
          element.into_owned()
        } else {
          start.into_owned()
        };
        writer
          .write_event(if empty { Event::Empty(element) } else { Event::Start(element) })
          .map_err(|error| invalid(&error.to_string()))?;
        if !empty {
          stack.push(name);
        }
      }
      Event::Text(text)
        if stack.last().is_some_and(|name| name == "lodLevel")
          && stack.get(stack.len().saturating_sub(2)).is_some_and(|name| name == "Item")
          && stack.get(stack.len().saturating_sub(3)).is_some_and(|name| name == "entities") =>
      {
        let entity = &entities[current.ok_or_else(|| invalid("clone entity context missing"))?];
        writer
          .write_event(Event::Text(BytesText::new(&entity.lod_level)))
          .map_err(|error| invalid(&error.to_string()))?;
        drop(text);
      }
      Event::End(end) => {
        writer
          .write_event(Event::End(end.into_owned()))
          .map_err(|error| invalid(&error.to_string()))?;
        stack.pop();
      }
      Event::Eof => break,
      event => {
        writer.write_event(event.into_owned()).map_err(|error| invalid(&error.to_string()))?
      }
    }
  }
  if count != entities.len() {
    return Err(invalid("clone entity count differs from original XML"));
  }
  String::from_utf8(writer.into_inner()).map_err(|_| invalid("clone XML is not UTF-8"))
}
