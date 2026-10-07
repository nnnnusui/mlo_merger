use crate::core::find::EntityQuery;
use bpaf::*;
use std::{io::Write, path::PathBuf};

/// Arguments for a read-only GUID or position search of merged native YMAPs.
#[derive(Debug, Clone)]
pub struct Find {
  /// GUID or inclusive three-dimensional radius condition.
  pub query: EntityQuery,
  /// Optional case-insensitive filename glob filter.
  pub filter: Option<String>,
  /// Include recorded pre-merge vanilla and source entities.
  pub diff_all: bool,
  /// Existing merged output directory.
  pub merged_dir: PathBuf,
}

/// Parses GUID and position searches without invoking any pipeline stages.
pub fn parser() -> impl Parser<Find> {
  let kind = long("find").argument::<String>("KIND").guard(
    |kind| kind == "entity-guid" || kind == "entity-position",
    "Use --find entity-guid or entity-position",
  );
  let merged_dir = short('i')
    .long("input")
    .help("Merged directory (default: asset/merged)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/merged"));
  let filter = long("filter")
    .help("Restrict YMAP filenames with a case-insensitive glob (quote wildcard patterns)")
    .argument::<String>("GLOB")
    .optional();
  let diff_all =
    long("diff-all").help("Also output recorded vanilla and source data before merging").switch();
  let radius = long("round")
    .help("Inclusive 3D radius for entity-position (default: 1.0)")
    .argument::<f64>("DISTANCE")
    .optional();
  let value = any::<String, _, _>("GUID|X,Y,Z", Some);
  construct!(kind, merged_dir, filter, diff_all, radius, value).parse(
    |(kind, merged_dir, filter, diff_all, radius, value)| {
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
        let radius = radius.unwrap_or(1.0);
        if !position.iter().all(|value| value.is_finite() && (*value as f32).is_finite())
          || !radius.is_finite()
          || radius < 0.0
        {
          return Err(
            "Position and --round must be finite; --round must be nonnegative".to_string(),
          );
        }
        EntityQuery::Position {
          position,
          radius,
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
