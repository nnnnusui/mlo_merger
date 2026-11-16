//! What stream file names identify across resources and vanilla.
//!
//! A stream file's name is how the game identifies it, so files of the same name in several
//! resources, or a resource's file named after a vanilla file, are one stream file shipped more
//! than once. Some names are the exception: every resource ships a file of that name for itself,
//! such as the `_manifest.ymf` declaring its own map dependencies. Those identify no stream file
//! across resources: they are not conflicts, replace no vanilla file and are no added stream file
//! several resources share, and each resource's is deployed as its own.

/// The lowercase file names each resource ships for itself.
const OWN_FILE_NAMES: &[&str] = &["_manifest.ymf"];

/// Whether `file_name` identifies one stream file across resources and vanilla; matched
/// case-insensitively.
pub fn identifies_stream_file(file_name: &str) -> bool {
  !OWN_FILE_NAMES.iter().any(|name| name.eq_ignore_ascii_case(file_name))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn manifests_identify_no_stream_file() {
    assert!(!identifies_stream_file("_manifest.ymf"));
    assert!(!identifies_stream_file("_MANIFEST.YMF"));
    assert!(identifies_stream_file("hei_ch1_11.ymap"));
    assert!(identifies_stream_file("other.ymf"));
  }
}
