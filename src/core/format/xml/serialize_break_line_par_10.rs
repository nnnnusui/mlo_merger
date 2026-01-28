use crate::return_early;

pub fn serialize_break_line_par_10<S>(
  x: &str,
  s: S,
) -> Result<S::Ok, S::Error>
where
  S: serde::Serializer,
{
  let items: Vec<&str> = x.split_whitespace().collect();
  let lines: Vec<String> = items.chunks(10).map(|chunk| chunk.join(" ")).collect();
  return_early!(if (lines.is_empty()) return s.serialize_str(""));

  let indent = "\n        ";
  let result = indent.to_string() + &lines.join(indent) + "\n    ";
  s.serialize_str(&result)
}
