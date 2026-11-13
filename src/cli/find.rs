use crate::core::find::{EntityQuery, InputStores};
use bpaf::*;
use std::path::{Path, PathBuf};

/// Arguments for a read-only search of merged native files and optional source inputs.
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
  /// Where the inputs `--all` reads are.
  pub stores: InputStores,
}

/// Parses find searches without invoking any pipeline stages.
pub fn parser(asset: &Path) -> impl Parser<Find> + use<> {
  options(asset)
    .to_options()
    .descr("Search merged files, and with --all their inputs, by entity GUID or position")
    .command("--find")
}

fn options(asset: &Path) -> impl Parser<Find> + use<> {
  let defaults = InputStores::under(asset);
  let source_dir = long("source")
    .help("Source resource directory, for --all (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(defaults.source_dir);
  let source_cache_dir = long("source-cache")
    .help("Source cache directory, for --all (default: asset/source-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(defaults.source_cache_dir);
  let vanilla_cache_dir = long("vanilla-cache")
    .help("Derived vanilla cache directory, for --all (default: asset/vanilla-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(defaults.vanilla_cache_dir);
  let stores = construct!(InputStores {
    source_dir,
    source_cache_dir,
    vanilla_cache_dir
  });
  let search = search_parser(asset);
  // The search ends with the positional GUID or position, which bpaf requires last.
  construct!(stores, search).map(|(stores, search)| Find {
    stores,
    ..search
  })
}

fn search_parser(asset: &Path) -> impl Parser<Find> + use<> {
  let kind = positional::<String>("KIND")
    .help("entity-guid, entity-position, ybn-position, file-from-position or occlude-position")
    .guard(
    |kind| {
      matches!(
        kind.as_str(),
        "entity-guid"
          | "entity-position"
          | "ybn-position"
          | "file-from-position"
          | "occlude-position"
      )
    },
      "Use --find entity-guid, entity-position, ybn-position, file-from-position or occlude-position",
    );
  let merged_dir = short('i')
    .long("input")
    .help("Merged directory (default: asset/merged)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("merged"));
  let filter = long("filter")
    .help("Restrict resource filenames with a case-insensitive glob (quote wildcard patterns)")
    .argument::<String>("GLOB")
    .optional();
  let all = long("all")
    .help("Search all available inputs, including vanilla and source data before merging")
    .switch();
  let radius = long("radius")
    .help(
      "Inclusive 3D radius for entity-position, ybn-position and occlude-position (default: 1.0)",
    )
    .argument::<f64>("DISTANCE")
    .optional();
  let shape_type = long("type")
    .help("YBN shape type: box, triangle, sphere, capsule or cylinder (default: all)")
    .argument::<String>("TYPE")
    .optional();
  // `any` takes values starting with `-`, such as negative coordinates, but leaves help.
  let value = any::<String, _, _>("GUID|X,Y,Z", |value| {
    (!matches!(value.as_str(), "-h" | "--help")).then_some(value)
  });
  // Replaced by the parsed stores in `parser`.
  let stores = InputStores::under(asset);
  construct!(merged_dir, filter, all, radius, shape_type, kind, value).parse(
    move |(merged_dir, filter, all, radius, shape_type, kind, value)| {
      if kind != "ybn-position" && shape_type.is_some() {
        return Err("--type applies only to ybn-position".to_string());
      }
      let query = if kind == "entity-guid" {
        if radius.is_some() {
          return Err("--radius applies only to position searches".to_string());
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
        if !position.iter().all(|value| value.is_finite() && (*value as f32).is_finite()) {
          return Err("Position coordinates must be finite and in range".to_string());
        }
        if kind == "file-from-position" {
          if radius.is_some() {
            return Err("--radius does not apply to file-from-position".to_string());
          }
          EntityQuery::FileFromPosition {
            position,
          }
        } else {
          let radius = radius.unwrap_or(1.0);
          if !radius.is_finite() || radius < 0.0 {
            return Err("Radius must be finite and nonnegative".to_string());
          }
          if kind == "occlude-position" {
            EntityQuery::OccludePosition {
              position,
              radius,
            }
          } else if kind == "ybn-position" {
            let kind = shape_type.map(|value| value.to_ascii_lowercase());
            if kind.as_ref().is_some_and(|value| {
              !matches!(value.as_str(), "box" | "triangle" | "sphere" | "capsule" | "cylinder")
            }) {
              return Err(
                "YBN type must be box, triangle, sphere, capsule or cylinder".to_string(),
              );
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
        }
      };
      Ok(Find {
        query,
        merged_dir,
        filter,
        diff_all: all,
        stores: stores.clone(),
      })
    },
  )
}

/// Writes only the search result XML to standard output.
pub fn run(command: Find) -> Result<String, Box<dyn std::error::Error>> {
  let xml = crate::core::find::FindEntity {
    query: command.query,
    merged_dir: command.merged_dir,
    stores: command.stores,
    filter: command.filter,
    diff_all: command.diff_all,
  }
  .run()?;
  Ok(format!("{xml}\n"))
}
