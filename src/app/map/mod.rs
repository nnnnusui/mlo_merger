//! Application workflows for maps and MLOs.

pub mod pipeline;
pub mod vanilla;

#[cfg(test)]
mod web_sample;

pub use pipeline::{Pipeline, PipelineChanges, PipelineReport, PipelineSummary};
pub use vanilla::GenerateVanilla;
