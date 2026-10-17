use crate::common::position::Position;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, struct_diff::Diff)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(attr(cfg_attr(feature = "typescript", derive(specta::Type))))]
pub struct Triangle {
  pub corner_1: Position,
  pub corner_2: Position,
  pub corner_3: Position,
}
