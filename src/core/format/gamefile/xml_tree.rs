use std::collections::HashMap;
use std::io;

use quick_xml::Reader;
use quick_xml::events::Event;

#[derive(Debug, Default)]
pub(crate) struct XmlElement {
  pub(crate) name: String,
  pub(crate) attributes: HashMap<String, String>,
  pub(crate) text: String,
  pub(crate) children: Vec<XmlElement>,
}

pub(crate) fn parse_xml(xml: &str) -> io::Result<XmlElement> {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  let mut stack: Vec<XmlElement> = Vec::new();
  let mut root = None;

  loop {
    match reader.read_event().map_err(|error| invalid_data(&error.to_string()))? {
      Event::Start(event) => {
        let mut node = XmlElement {
          name: String::from_utf8_lossy(event.name().as_ref()).into_owned(),
          ..XmlElement::default()
        };
        for attribute in event.attributes() {
          let attribute = attribute.map_err(|error| invalid_data(&error.to_string()))?;
          let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
          let value = attribute
            .decode_and_unescape_value(reader.decoder())
            .map_err(|error| invalid_data(&error.to_string()))?
            .into_owned();
          node.attributes.insert(key, value);
        }
        stack.push(node);
      }
      Event::Empty(event) => {
        let mut node = XmlElement {
          name: String::from_utf8_lossy(event.name().as_ref()).into_owned(),
          ..XmlElement::default()
        };
        for attribute in event.attributes() {
          let attribute = attribute.map_err(|error| invalid_data(&error.to_string()))?;
          let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
          let value = attribute
            .decode_and_unescape_value(reader.decoder())
            .map_err(|error| invalid_data(&error.to_string()))?
            .into_owned();
          node.attributes.insert(key, value);
        }
        attach_node(&mut stack, &mut root, node)?;
      }
      Event::Text(event) => {
        if let Some(node) = stack.last_mut() {
          node.text.push_str(&event.decode().map_err(|error| invalid_data(&error.to_string()))?);
        }
      }
      Event::End(_) => {
        let node = stack.pop().ok_or_else(|| invalid_data("XML element nesting is invalid"))?;
        attach_node(&mut stack, &mut root, node)?;
      }
      Event::Eof => break,
      _ => {}
    }
  }
  root.ok_or_else(|| invalid_data("XML has no root element"))
}

fn attach_node(
  stack: &mut [XmlElement],
  root: &mut Option<XmlElement>,
  node: XmlElement,
) -> io::Result<()> {
  if let Some(parent) = stack.last_mut() {
    parent.children.push(node);
  } else if root.replace(node).is_some() {
    return Err(invalid_data("XML has multiple root elements"));
  }
  Ok(())
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}
