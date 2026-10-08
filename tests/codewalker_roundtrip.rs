//! Round-trip smoke test for the CodeWalker.Bridge hosting layer.
//!
//! Ignored by default since it requires a locally built `bridge/CodeWalker.Bridge`
//! (see build.rs / CODEWALKER_CORE_DLL). Run with:
//!   CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll cargo test --test codewalker_roundtrip -- --ignored

use std::path::Path;

#[path = "../src/core/format/gamefile/test/compare.rs"]
mod compare;
use compare::{
  assert_canonical_xml_eq, assert_ybn_xml_eq, canonical_xml, describe_difference, truncate,
  ybn_resource_quantums,
};

use mlo_merger::core::codewalker::CodeWalker;
use mlo_merger::core::format::gamefile::{
  meta_resource::{MetaResource, MetaSchemaCatalog},
  meta_xml::ymap_to_xml,
  resource_convert::{NativeResourceFormat, resource_to_xml, xml_to_resource},
  resource_file::Rsc7Resource,
};
use mlo_merger::core::format::ymap::xml::XmlYmap;
use mlo_merger::core::merge::{run::MergeYmap, ybn_conflicts::MergeYbnConflicts};
use mlo_merger::core::xmlconvert::Xml2Ymap;

#[path = "codewalker_roundtrip/archive.rs"]
mod archive;
#[path = "codewalker_roundtrip/diagnostics.rs"]
mod diagnostics;
#[path = "codewalker_roundtrip/formats.rs"]
mod formats;
#[path = "codewalker_roundtrip/helpers.rs"]
mod helpers;
#[path = "codewalker_roundtrip/pso.rs"]
mod pso;
#[path = "codewalker_roundtrip/ybn.rs"]
mod ybn;
#[path = "codewalker_roundtrip/ymap.rs"]
mod ymap;

use helpers::*;
