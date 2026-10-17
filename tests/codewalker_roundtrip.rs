//! Round-trip smoke test for the CodeWalker.Bridge hosting layer.
//!
//! Ignored by default since it requires a locally built `bridge/CodeWalker.Bridge`
//! (see build.rs / CODEWALKER_CORE_DLL). Run with:
//!   CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll cargo test --test codewalker_roundtrip -- --ignored

use std::path::Path;

#[path = "codewalker_roundtrip/compare.rs"]
mod compare;
use compare::{
  assert_canonical_xml_eq, assert_ybn_xml_eq, canonical_xml, describe_difference, truncate,
  ybn_resource_quantums,
};

use mlo_merger::core::codewalker::CodeWalker;
use mlo_merger::core::format::gamefile::{
  meta::{MetaResource, MetaSchemaCatalog},
  rsc7::Rsc7Resource,
};
use mlo_merger::core::format::stream_file::{FileType, StreamFile};
use mlo_merger::core::format::ymap::{binary::ymap_to_model, model::ymap::Ymap};
use mlo_merger::core::merge::{run::MergeYmap, ybn_conflicts::MergeYbnConflicts};

#[path = "codewalker_roundtrip/archive.rs"]
mod archive;
#[path = "codewalker_roundtrip/diagnostics.rs"]
mod diagnostics;
#[path = "codewalker_roundtrip/helpers.rs"]
mod helpers;
#[path = "codewalker_roundtrip/pso.rs"]
mod pso;
#[path = "codewalker_roundtrip/ybn.rs"]
mod ybn;
#[path = "codewalker_roundtrip/ymap.rs"]
mod ymap;

use helpers::*;
