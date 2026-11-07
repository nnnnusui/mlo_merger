//! Runs the executable as a host resource does: resources under a root outside the working
//! directory, and the host's own `asset/` holding the configuration and vanilla archive.
//!
//! The layout is the one `scripts/test_web.mjs` runs the browser build in:
//!
//! ```text
//! <root>/{the sample's resources}
//! <root>/[n4u]/n4u_mlomerger/          working directory
//!   asset/config.toml, asset/vanilla/  from web/sample
//! <root>/[n4u]/mlo_merged/             deployment output
//! ```

use std::{
  fs,
  path::{Path, PathBuf},
  process::Command,
};

fn copy_dir(
  from: &Path,
  to: &Path,
) {
  fs::create_dir_all(to).unwrap();
  for entry in fs::read_dir(from).unwrap() {
    let entry = entry.unwrap();
    let destination = to.join(entry.file_name());
    if entry.file_type().unwrap().is_dir() {
      copy_dir(&entry.path(), &destination);
    } else {
      fs::copy(entry.path(), destination).unwrap();
    }
  }
}

/// Runs the executable in `cwd` and returns its standard output, failing on an error exit.
fn run(
  cwd: &Path,
  args: &[&str],
) -> String {
  let output = Command::new(env!("CARGO_BIN_EXE_mlo_merger"))
    .args(args)
    .current_dir(cwd)
    .env("RUST_LOG", "warn")
    .output()
    .unwrap();
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(output.status.success(), "{args:?} failed:\n{stderr}");
  String::from_utf8(output.stdout).unwrap()
}

/// The one line of standard output, which ends in a newline.
fn line(stdout: &str) -> &str {
  let line = stdout.strip_suffix('\n').expect("Output ends in a newline");
  assert!(!line.contains('\n'), "Output is one line: {stdout}");
  line
}

#[test]
fn runs_with_resources_outside_the_working_directory() {
  let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/sample/asset");
  let temporary =
    std::env::temp_dir().join(format!("mlo_merger_host_layout_{}", std::process::id()));
  let _ = fs::remove_dir_all(&temporary);
  let root = temporary.join("txData/preview/resources");
  let host = root.join("[n4u]/n4u_mlomerger");
  let merged: PathBuf = root.join("[n4u]/mlo_merged");
  copy_dir(&sample.join("source"), &root);
  copy_dir(&sample.join("vanilla"), &host.join("asset/vanilla"));
  fs::copy(sample.join("config.toml"), host.join("asset/config.toml")).unwrap();
  fs::write(host.join("fxmanifest.lua"), "fx_version 'cerulean'\n").unwrap();
  let (root_arg, merged_arg) = (root.to_str().unwrap(), merged.to_str().unwrap());
  let pipeline = [
    "--pipeline",
    "--yes",
    "--input",
    root_arg,
    "--output",
    merged_arg,
    "--exclude",
    "smpl_custom_props,mlo_merged",
    "--gamebuild",
    "100",
  ];

  let report: serde_json::Value = serde_json::from_str(line(&run(&host, &pipeline))).unwrap();
  assert!(!report["merged"]["added"].as_array().unwrap().is_empty(), "{report}");
  assert!(merged.join("stream/ymap/merged/smpl_block_01.ymap").is_file());
  assert!(!root.join("smpl_block_rework/stream/maps/smpl_block_01.ymap").exists());
  // The deployed resource lies under the input root; a second run excludes it and changes nothing.
  let again: serde_json::Value = serde_json::from_str(line(&run(&host, &pipeline))).unwrap();
  assert_eq!(again["merged"]["added"], serde_json::json!([]), "{again}");

  let restored = run(&host, &["--restore-source-cache", "--input", root_arg, "smpl_block_rework"]);
  assert_eq!(restored, "Restored 3 source files.\n");
  assert!(root.join("smpl_block_rework/stream/maps/smpl_block_01.ymap").is_file());

  let read = run(&host, &["--read", "--source", root_arg, "smpl_block_01.ymap"]);
  let read: serde_json::Value = serde_json::from_str(line(&read)).unwrap();
  let files = read["files"].as_array().unwrap();
  assert!(files.iter().any(|file| file["origin"]["Resource"]["resource"] == "smpl_block_rework"));
  fs::remove_dir_all(temporary).unwrap();
}
