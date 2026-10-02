use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
  println!("cargo:rerun-if-changed=bridge/CodeWalker.Bridge");
  println!("cargo:rerun-if-env-changed=CODEWALKER_CORE_DLL");

  let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
  let bridge_dir = manifest_dir.join("bridge/CodeWalker.Bridge");
  let publish_dir = bridge_dir.join("bin/publish");

  if Command::new("dotnet").arg("--version").output().is_err() {
    println!(
      "cargo:warning=`dotnet` was not found on PATH. Install the .NET SDK to enable the CodeWalker.Bridge (ymap<->xml conversion / default pipeline command)."
    );
    return;
  }

  let dll_path = match env::var("CODEWALKER_CORE_DLL") {
    Ok(path) => PathBuf::from(path),
    // default convention: <workspace>/CodeWalker.Core.dll, workspace defaulting to "asset"
    Err(_) => manifest_dir.join("asset/CodeWalker.Core.dll"),
  };
  if !dll_path.exists() {
    println!(
      "cargo:warning=CodeWalker.Core.dll not found at {} (set CODEWALKER_CORE_DLL, or place it there). It is not distributed with this repo (GPL-3.0). The default pipeline command will not be available.",
      dll_path.display()
    );
    return;
  }

  let status = Command::new("dotnet")
    .arg("publish")
    .arg(&bridge_dir)
    .arg("-c")
    .arg("Release")
    .arg("-o")
    .arg(&publish_dir)
    .arg(format!("-p:CodeWalkerCoreDllPath={}", dll_path.display()))
    .status()
    .expect("failed to run `dotnet publish`");

  if !status.success() {
    println!("cargo:warning=`dotnet publish` for CodeWalker.Bridge failed (exit code {status})");
  }
}
