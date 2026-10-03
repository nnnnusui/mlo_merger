use mlo_merger::core::format::gamefile::resource_file::Rsc7Resource;
use quick_xml::{Reader, events::Event};
use std::collections::BTreeMap;

pub(crate) fn ybn_resource_quantums(bytes: &[u8]) -> Result<Vec<[f32; 3]>, String> {
  let resource = Rsc7Resource::decode(bytes).map_err(|error| error.to_string())?;
  fn visit(
    resource: &Rsc7Resource,
    pointer: u64,
    output: &mut Vec<[f32; 3]>,
  ) -> Result<(), String> {
    if pointer == 0 {
      return Ok(());
    }
    let common = resource.read_address(pointer, 112).map_err(|error| error.to_string())?;
    match common[16] {
      10 => {
        let composite = resource.read_address(pointer, 176).map_err(|error| error.to_string())?;
        let children_pointer = u64::from_le_bytes(composite[112..120].try_into().unwrap());
        let count = u16::from_le_bytes(composite[160..162].try_into().unwrap()) as usize;
        if count > 0 {
          let children = resource
            .read_address(children_pointer, count * 8)
            .map_err(|error| error.to_string())?;
          for child in children.chunks_exact(8) {
            visit(resource, u64::from_le_bytes(child.try_into().unwrap()), output)?;
          }
        }
      }
      4 | 8 => {
        let size = if common[16] == 8 { 336 } else { 304 };
        let geometry = resource.read_address(pointer, size).map_err(|error| error.to_string())?;
        if u32::from_le_bytes(geometry[208..212].try_into().unwrap()) > 0 {
          output.push([
            f32::from_le_bytes(geometry[144..148].try_into().unwrap()),
            f32::from_le_bytes(geometry[148..152].try_into().unwrap()),
            f32::from_le_bytes(geometry[152..156].try_into().unwrap()),
          ]);
        }
      }
      _ => {}
    }
    Ok(())
  }
  let mut output = Vec::new();
  visit(&resource, 0x5000_0000, &mut output)?;
  Ok(output)
}

pub(crate) fn truncate(value: &str) -> &str {
  value.get(..value.floor_char_boundary(240)).unwrap_or(value)
}

pub(crate) fn assert_canonical_xml_eq(
  actual: &str,
  expected: &str,
  context: &str,
) -> Result<(), String> {
  if context.starts_with("Native export ") && context.ends_with(".ybn") {
    return assert_ybn_xml_eq(actual, expected, context, false, None);
  }
  let actual_events = canonical_xml(actual);
  let expected_events = canonical_xml(expected);
  for (index, (actual_event, expected_event)) in
    actual_events.iter().zip(&expected_events).enumerate()
  {
    if actual_event != expected_event {
      let start = index.saturating_sub(3);
      let token_difference = actual_event
        .split_whitespace()
        .zip(expected_event.split_whitespace())
        .enumerate()
        .find(|(_, (actual_token, expected_token))| actual_token != expected_token)
        .map(|(token_index, (actual_token, expected_token))| {
          format!(" token {token_index}: Native={actual_token}, CodeWalker={expected_token}")
        })
        .unwrap_or_default();
      let actual_context = actual_events[start..(index + 4).min(actual_events.len())]
        .iter()
        .map(|event| truncate(event))
        .collect::<Vec<_>>()
        .join(" | ");
      let expected_context = expected_events[start..(index + 4).min(expected_events.len())]
        .iter()
        .map(|event| truncate(event))
        .collect::<Vec<_>>()
        .join(" | ");
      return Err(format!(
        "{context} differs at XML event {index}:{token_difference}; Native='{}' ({}), CodeWalker='{}' ({}); Native context=[{}]; CodeWalker context=[{}]; {}",
        truncate(actual_event),
        truncate(&xml_event_at(actual, index)),
        truncate(expected_event),
        truncate(&xml_event_at(expected, index)),
        actual_context,
        expected_context,
        describe_difference(actual_event, expected_event)
      ));
    }
  }
  if actual_events.len() != expected_events.len() {
    return Err(format!(
      "{context} has {} XML events; CodeWalker has {}",
      actual_events.len(),
      expected_events.len()
    ));
  }
  Ok(())
}

type YbnQuantumPair<'a> = (&'a [[f32; 3]], &'a [[f32; 3]]);

pub(crate) fn assert_ybn_xml_eq(
  actual: &str,
  expected: &str,
  context: &str,
  ignore_polygon_order: bool,
  binary_quantums: Option<YbnQuantumPair<'_>>,
) -> Result<(), String> {
  let native = canonical_ybn_xml(actual, ignore_polygon_order)?;
  let reference = canonical_ybn_xml(expected, ignore_polygon_order)?;
  if native != reference {
    let index = native
      .iter()
      .zip(&reference)
      .position(|(a, b)| a != b)
      .unwrap_or(native.len().min(reference.len()));
    return Err(format!("{context} differs in non-vertex YBN data at canonical event {index}"));
  }

  let actual_vertices = ybn_vertex_arrays(actual)?;
  let expected_vertices = ybn_vertex_arrays(expected)?;
  let (actual_quanta, expected_quanta): YbnQuantumPair<'_> = match binary_quantums {
    Some(quantums) => quantums,
    None if !ignore_polygon_order => (&[], &[]),
    None => return Err(format!("{context} requires binary Quantum values for rebuild comparison")),
  };
  if actual_vertices.len() != expected_vertices.len() {
    return Err(format!("{context} has a different number of Vertices arrays"));
  }
  for (array_index, (native, codewalker)) in
    actual_vertices.iter().zip(&expected_vertices).enumerate()
  {
    if native.len() != codewalker.len() {
      return Err(format!("{context} Vertices array {array_index} has different lengths"));
    }
    for (index, (a, b)) in native.iter().zip(codewalker).enumerate() {
      let differs = if ignore_polygon_order {
        let quantum = actual_quanta
          .get(array_index)
          .ok_or_else(|| format!("{context} lacks Native geometry quantum"))?;
        let reference_quantum = expected_quanta
          .get(array_index)
          .ok_or_else(|| format!("{context} lacks CodeWalker geometry quantum"))?;
        let epsilon = f32::EPSILON * a.abs().max(b.abs()).max(1.0) * 2.0;
        (a - b).abs() > quantum[index % 3].max(reference_quantum[index % 3]) + epsilon
      } else {
        a.to_bits() != b.to_bits()
      };
      if differs {
        let native_quantum = actual_quanta.get(array_index).copied().unwrap_or([0.0; 3]);
        let codewalker_quantum = expected_quanta.get(array_index).copied().unwrap_or([0.0; 3]);
        return Err(format!(
          "{context} Vertices array {array_index} coordinate {index} differs: Native={a}, CodeWalker={b}; quantum Native={native_quantum:?}, CodeWalker={codewalker_quantum:?}; coordinate counts Native={}, CodeWalker={}",
          native.len(),
          codewalker.len()
        ));
      }
    }
  }
  Ok(())
}

fn ybn_vertex_arrays(xml: &str) -> Result<Vec<Vec<f32>>, String> {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  let mut inside_vertices = false;
  let mut values = Vec::new();
  let mut arrays = Vec::new();
  loop {
    match reader.read_event().map_err(|error| error.to_string())? {
      Event::Start(element) if element.name().as_ref() == b"Vertices" => {
        inside_vertices = true;
        values.clear();
      }
      Event::Text(text) if inside_vertices => {
        let decoded = text.decode().map_err(|error| error.to_string())?;
        for token in decoded
          .split(|character: char| character.is_whitespace() || character == ',')
          .filter(|token| !token.is_empty())
        {
          values.push(
            token.parse::<f32>().map_err(|_| format!("invalid YBN vertex coordinate {token}"))?,
          );
        }
      }
      Event::End(element) if element.name().as_ref() == b"Vertices" => {
        arrays.push(std::mem::take(&mut values));
        inside_vertices = false;
      }
      Event::Eof => break,
      _ => {}
    }
  }
  Ok(arrays)
}

#[derive(Default)]
struct CompareXmlNode {
  name: String,
  attributes: BTreeMap<String, String>,
  text: String,
  children: Vec<CompareXmlNode>,
}

fn canonical_ybn_xml(
  xml: &str,
  ignore_polygon_order: bool,
) -> Result<Vec<String>, String> {
  let root = parse_compare_xml(xml)?;
  Ok(vec![canonical_ybn_node(&root, ignore_polygon_order)?])
}

fn parse_compare_xml(xml: &str) -> Result<CompareXmlNode, String> {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  let mut stack = Vec::<CompareXmlNode>::new();
  let mut root = None;
  loop {
    match reader.read_event().map_err(|error| error.to_string())? {
      Event::Start(element) => stack.push(compare_xml_element(&reader, &element)?),
      Event::Empty(element) => {
        attach_compare_node(compare_xml_element(&reader, &element)?, &mut stack, &mut root)?
      }
      Event::Text(text) => {
        if let Some(node) = stack.last_mut() {
          node.text.push_str(&text.decode().map_err(|error| error.to_string())?);
        }
      }
      Event::End(_) => {
        let node = stack.pop().ok_or_else(|| "invalid YBN XML nesting".to_string())?;
        attach_compare_node(node, &mut stack, &mut root)?;
      }
      Event::Eof => break,
      _ => {}
    }
  }
  root.ok_or_else(|| "YBN XML has no root element".into())
}

fn compare_xml_element(
  reader: &Reader<&[u8]>,
  element: &quick_xml::events::BytesStart<'_>,
) -> Result<CompareXmlNode, String> {
  let mut node = CompareXmlNode {
    name: String::from_utf8_lossy(element.name().as_ref()).into_owned(),
    ..CompareXmlNode::default()
  };
  for attribute in element.attributes() {
    let attribute = attribute.map_err(|error| error.to_string())?;
    node.attributes.insert(
      String::from_utf8_lossy(attribute.key.as_ref()).into_owned(),
      attribute
        .decode_and_unescape_value(reader.decoder())
        .map_err(|error| error.to_string())?
        .into_owned(),
    );
  }
  Ok(node)
}

fn attach_compare_node(
  node: CompareXmlNode,
  stack: &mut [CompareXmlNode],
  root: &mut Option<CompareXmlNode>,
) -> Result<(), String> {
  if let Some(parent) = stack.last_mut() {
    parent.children.push(node);
  } else if root.replace(node).is_some() {
    return Err("YBN XML has multiple root elements".into());
  }
  Ok(())
}

fn canonical_ybn_node(
  node: &CompareXmlNode,
  ignore_polygon_order: bool,
) -> Result<String, String> {
  if node.name == "Vertices" {
    return Ok("Vertices<quantized>".into());
  }
  let material_keys = node
    .children
    .iter()
    .find(|child| child.name == "Materials")
    .map(|materials| {
      materials
        .children
        .iter()
        .filter(|item| item.name == "Item")
        .map(canonical_compare_node)
        .collect::<Vec<_>>()
    })
    .unwrap_or_default();
  let mut children = Vec::new();
  for child in &node.children {
    match child.name.as_str() {
      "Vertices" => children.push("Vertices<quantized>".into()),
      "Materials" => {
        let mut values = material_keys.clone();
        values.sort();
        children.push(format!("Materials[{}]", values.join("|")));
      }
      "Polygons" => {
        let mut polygons = Vec::new();
        for polygon in &child.children {
          let material_index = polygon
            .attributes
            .get("m")
            .ok_or_else(|| "YBN polygon has no material index".to_string())?
            .parse::<usize>()
            .map_err(|_| "YBN polygon material index is invalid".to_string())?;
          let material = material_keys.get(material_index).ok_or_else(|| {
            format!("YBN polygon material index {material_index} is out of range")
          })?;
          polygons.push(canonical_compare_node_without_material_index(polygon, material));
        }
        if ignore_polygon_order {
          polygons.sort();
        }
        children.push(format!("Polygons[{}]", polygons.join("|")));
      }
      _ => children.push(canonical_ybn_node(child, ignore_polygon_order)?),
    }
  }
  let mut attributes = node
    .attributes
    .iter()
    .map(|(key, value)| format!("{key}={}", normalize_value(value)))
    .collect::<Vec<_>>();
  attributes.sort();
  let text = node
    .text
    .split(|character: char| character.is_whitespace() || character == ',')
    .filter(|part| !part.is_empty())
    .map(normalize_value)
    .collect::<Vec<_>>()
    .join(" ");
  Ok(format!(
    "{}({})[{}]{{{}}}",
    normalize_element_name(&node.name),
    attributes.join(" "),
    text,
    children.join(";")
  ))
}

fn canonical_compare_node(node: &CompareXmlNode) -> String {
  canonical_compare_node_without_material_index(node, "")
}

fn canonical_compare_node_without_material_index(
  node: &CompareXmlNode,
  material: &str,
) -> String {
  let mut attributes = node
    .attributes
    .iter()
    .filter(|(key, _)| key.as_str() != "m")
    .map(|(key, value)| format!("{key}={}", normalize_value(value)))
    .collect::<Vec<_>>();
  attributes.sort();
  let text = node
    .text
    .split(|character: char| character.is_whitespace() || character == ',')
    .filter(|part| !part.is_empty())
    .map(normalize_value)
    .collect::<Vec<_>>()
    .join(" ");
  format!(
    "{}({})[{}]{{{}}}<material:{material}>",
    normalize_element_name(&node.name),
    attributes.join(" "),
    text,
    node.children.iter().map(canonical_compare_node).collect::<Vec<_>>().join(";")
  )
}

fn xml_event_at(
  xml: &str,
  target: usize,
) -> String {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  let mut index = 0;
  let mut stack = Vec::new();
  loop {
    let event = match reader.read_event() {
      Ok(event) => event,
      Err(error) => return format!("XML parse error: {error}"),
    };
    let label = match event {
      Event::Start(element) => {
        let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
        let label = format!("start:{}/{}", stack.join("/"), name);
        stack.push(name);
        Some(label)
      }
      Event::Empty(element) => Some(format!(
        "empty:{}/{}",
        stack.join("/"),
        String::from_utf8_lossy(element.name().as_ref())
      )),
      Event::End(element) => {
        let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
        let label = format!("end:{}/{}", stack.join("/"), name);
        stack.pop();
        Some(label)
      }
      Event::Text(text) => {
        let value = text.decode().unwrap_or_default();
        let value = value.trim();
        (!value.is_empty()).then(|| format!("text:{}/{}", stack.join("/"), value))
      }
      Event::Eof => return "EOF".to_string(),
      _ => None,
    };
    if let Some(label) = label {
      if index == target {
        return label;
      }
      index += 1;
    }
  }
}

pub(crate) fn describe_difference(
  actual: &str,
  expected: &str,
) -> String {
  let offset = actual
    .chars()
    .zip(expected.chars())
    .position(|(actual, expected)| actual != expected)
    .unwrap_or_else(|| actual.len().min(expected.len()));
  let start = offset.saturating_sub(60);
  let actual_end = (offset + 100).min(actual.len());
  let expected_end = (offset + 100).min(expected.len());
  format!(
    "first text offset {offset}; native='{}', source='{}'",
    actual.get(start..actual_end).unwrap_or(actual),
    expected.get(start..expected_end).unwrap_or(expected)
  )
}

pub(crate) fn canonical_xml(xml: &str) -> Vec<String> {
  let mut normalized = Vec::new();
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  loop {
    match reader.read_event().unwrap() {
      Event::Start(element) => {
        normalized.push(format!("start:{}", canonical_element(&reader, &element)))
      }
      Event::Empty(element) => {
        normalized.push(format!("empty:{}", canonical_element(&reader, &element)))
      }
      Event::End(element) => {
        let name = normalize_element_name(&String::from_utf8_lossy(element.name().as_ref()));
        let start_prefix = format!("start:{name}");
        if normalized.last().is_some_and(|event| {
          event == &start_prefix || event.starts_with(&format!("{start_prefix} "))
        }) {
          let start = normalized.pop().unwrap();
          normalized.push(start.replacen("start:", "empty:", 1));
        } else {
          normalized.push(format!("end:{name}"));
        }
      }
      Event::Text(text) => {
        let value = text
          .decode()
          .unwrap()
          .split_whitespace()
          .map(normalize_value)
          .collect::<Vec<_>>()
          .join(" ");
        if !value.is_empty() {
          normalized.push(format!("text:{value}"));
        }
      }
      Event::Eof => break,
      _ => {}
    }
  }
  normalized
}

fn canonical_element(
  reader: &Reader<&[u8]>,
  element: &quick_xml::events::BytesStart<'_>,
) -> String {
  let mut attributes = element
    .attributes()
    .map(|attribute| {
      let attribute = attribute.unwrap();
      let name = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
      let value = normalize_value(&attribute.decode_and_unescape_value(reader.decoder()).unwrap());
      (name, value)
    })
    .collect::<Vec<_>>();
  attributes.sort();
  format!(
    "{}{}",
    normalize_element_name(&String::from_utf8_lossy(element.name().as_ref())),
    attributes.into_iter().map(|(name, value)| format!(" {name}={value}")).collect::<String>()
  )
}

fn normalize_element_name(name: &str) -> String {
  let reserved_type = match name {
    "STRING" => Some(0x10),
    "BYTE" => Some(0x11),
    "USHORT" => Some(0x13),
    "UINT" => Some(0x15),
    "FLOAT" => Some(0x21),
    "VECTOR4" => Some(0x33),
    "hash" => Some(0x4a),
    "POINTER" => Some(0x07),
    "ARRAYINFO" => Some(0x100),
    _ => None,
  };
  if let Some(hash) = reserved_type {
    return format!("hash:{hash:08X}");
  }
  if let Some(hash) = name.strip_prefix("hash_")
    && let Ok(hash) = u32::from_str_radix(hash, 16)
  {
    return format!("hash:{hash:08X}");
  }
  format!("hash:{:08X}", jenk_hash(name))
}

fn normalize_value(value: &str) -> String {
  let value = value.trim_end_matches(',');
  if let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X"))
    && let Ok(number) = u64::from_str_radix(hex, 16)
  {
    return number.to_string();
  }
  if let Some(hash) = value.strip_prefix("hash_")
    && let Ok(hash) = u32::from_str_radix(hash, 16)
  {
    return format!("hash:{hash:08X}");
  }
  if let Ok(number) = value.parse::<f32>() {
    if value.contains(['.', 'e', 'E']) || value.len() > 15 {
      return format!("f32:{:08X}", number.to_bits());
    }
    return value.to_string();
  }
  format!("hash:{:08X}", jenk_hash(value))
}

fn jenk_hash(value: &str) -> u32 {
  let mut hash = 0u32;
  for byte in value.bytes() {
    hash = hash.wrapping_add(byte as u32);
    hash = hash.wrapping_add(hash << 10);
    hash ^= hash >> 6;
  }
  hash = hash.wrapping_add(hash << 3);
  hash ^= hash >> 11;
  hash.wrapping_add(hash << 15)
}

#[test]
fn hexadecimal_and_decimal_integers_compare_equally() {
  assert_canonical_xml_eq(
    "<Item><color value=\"4294967295\" /></Item>",
    "<Item><color value=\"0xFFFFFFFF\" /></Item>",
    "color representation",
  )
  .unwrap();
  assert!(
    assert_canonical_xml_eq(
      "<Item><color value=\"4294967294\" /></Item>",
      "<Item><color value=\"0xFFFFFFFF\" /></Item>",
      "different color",
    )
    .is_err()
  );
}

#[test]
fn hash_names_and_float_spelling_preserve_comparison_semantics() {
  assert_canonical_xml_eq(
    "<Item><name>example</name><position x=\"1.5\" /></Item>",
    &format!(
      "<Item><name>hash_{:08X}</name><position x=\"1.5000\" /></Item>",
      jenk_hash("example")
    ),
    "equivalent names and floats",
  )
  .unwrap();
  assert!(
    assert_canonical_xml_eq(
      "<Item><position x=\"1.5\" /></Item>",
      "<Item><position x=\"1.6\" /></Item>",
      "changed float",
    )
    .is_err()
  );
}
