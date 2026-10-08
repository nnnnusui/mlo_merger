use crate::core::find::EntityQuery;
use bpaf::*;
use std::{io::Write, path::PathBuf};

/// Arguments for a read-only entity or collision search of merged native files.
#[derive(Debug, Clone)]
pub struct Find {
  /// GUID or inclusive three-dimensional entity/collision radius condition.
  pub query: EntityQuery,
  /// Optional case-insensitive filename glob filter.
  pub filter: Option<String>,
  /// Include recorded pre-merge vanilla and source data.
  pub diff_all: bool,
  /// Existing merged output directory.
  pub merged_dir: PathBuf,
}

/// Parses entity and collision searches without invoking any pipeline stages.
pub fn parser() -> impl Parser<Find> {
  let kind = long("find").argument::<String>("KIND").guard(
    |kind| matches!(kind.as_str(), "entity-guid" | "entity-position" | "ybn-position"),
    "Use --find entity-guid, entity-position or ybn-position",
  );
  let merged_dir = short('i')
    .long("input")
    .help("Merged directory (default: asset/merged)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/merged"));
  let filter = long("filter")
    .help("Restrict resource filenames with a case-insensitive glob (quote wildcard patterns)")
    .argument::<String>("GLOB")
    .optional();
  let diff_all =
    long("diff-all").help("Also output recorded vanilla and source data before merging").switch();
  let radius = long("round")
    .help("Inclusive 3D radius for entity-position (default: 1.0)")
    .argument::<f64>("DISTANCE")
    .optional();
  let ybn_radius = long("radius")
    .help("Inclusive 3D radius for ybn-position (default: 1.0)")
    .argument::<f64>("DISTANCE")
    .optional();
  let shape_type = long("type")
    .help("YBN shape type: box, triangle, sphere, capsule or cylinder (default: all)")
    .argument::<String>("TYPE")
    .optional();
  let value = any::<String, _, _>("GUID|X,Y,Z", Some);
  construct!(kind, merged_dir, filter, diff_all, radius, ybn_radius, shape_type, value).parse(
    |(kind, merged_dir, filter, diff_all, radius, ybn_radius, shape_type, value)| {
      if kind != "ybn-position" && (ybn_radius.is_some() || shape_type.is_some()) {
        return Err("--radius and --type apply only to ybn-position".to_string());
      }
      if kind == "ybn-position" && radius.is_some() {
        return Err(
          "Use --radius for ybn-position; --round applies only to entity-position".to_string(),
        );
      }
      let query = if kind == "entity-guid" {
        if radius.is_some() {
          return Err("--round applies only to entity-position".to_string());
        }
        EntityQuery::Guid(
          value
            .parse::<u32>()
            .map_err(|_| "GUID must be an unsigned 32-bit integer".to_string())?,
        )
      } else {
        let coordinates = value
          .split(',')
          .map(|coordinate| {
            coordinate.trim().parse::<f64>().map_err(|_| "Position must be X,Y,Z".to_string())
          })
          .collect::<Result<Vec<_>, _>>()?;
        let position: [f64; 3] = coordinates
          .try_into()
          .map_err(|_| "Position must contain exactly three coordinates".to_string())?;
        let radius = ybn_radius.or(radius).unwrap_or(1.0);
        if !position.iter().all(|value| value.is_finite() && (*value as f32).is_finite())
          || !radius.is_finite()
          || radius < 0.0
        {
          return Err("Position and radius must be finite; radius must be nonnegative".to_string());
        }
        if kind == "ybn-position" {
          let kind = shape_type.map(|value| value.to_ascii_lowercase());
          if kind.as_ref().is_some_and(|value| {
            !matches!(value.as_str(), "box" | "triangle" | "sphere" | "capsule" | "cylinder")
          }) {
            return Err("YBN type must be box, triangle, sphere, capsule or cylinder".to_string());
          }
          EntityQuery::YbnPosition {
            position,
            radius,
            kind,
          }
        } else {
          EntityQuery::Position {
            position,
            radius,
          }
        }
      };
      Ok(Find {
        query,
        merged_dir,
        filter,
        diff_all,
      })
    },
  )
}

/// Writes only the search result XML to standard output.
pub fn run(command: Find) -> Result<(), Box<dyn std::error::Error>> {
  let xml = crate::core::find::FindEntity {
    query: command.query,
    merged_dir: command.merged_dir,
    filter: command.filter,
    diff_all: command.diff_all,
  }
  .run()?;
  let mut stdout = std::io::stdout().lock();
  stdout.write_all(xml.as_bytes())?;
  stdout.write_all(b"\n")?;
  stdout.flush()?;
  Ok(())
}
