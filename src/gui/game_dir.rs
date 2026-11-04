//! Finding and checking a GTA V Legacy installation.

use std::path::{Path, PathBuf};

/// What a chosen directory looks like to vanilla generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameDirStatus {
  /// Contains `GTA5.exe` and `update/update.rpf`.
  Legacy,
  /// Contains only `GTA5_Enhanced.exe`, which is not supported.
  Enhanced,
  /// Not a GTA V directory.
  Unknown,
}

/// Classifies `dir` by the files vanilla generation requires.
pub fn status(dir: &Path) -> GameDirStatus {
  if dir.join("GTA5.exe").is_file() && dir.join("update/update.rpf").is_file() {
    GameDirStatus::Legacy
  } else if dir.join("GTA5_Enhanced.exe").is_file() {
    GameDirStatus::Enhanced
  } else {
    GameDirStatus::Unknown
  }
}

/// The first GTA V Legacy installation found in the usual install locations.
pub fn detect() -> Option<PathBuf> {
  candidates().into_iter().find(|dir| status(dir) == GameDirStatus::Legacy)
}

#[cfg(windows)]
fn candidates() -> Vec<PathBuf> {
  use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

  let mut dirs = Vec::new();
  let rockstar = r"SOFTWARE\WOW6432Node\Rockstar Games";
  for (key, value) in [
    (format!(r"{rockstar}\Grand Theft Auto V"), "InstallFolder"),
    (format!(r"{rockstar}\GTAV"), "InstallFolderSteam"),
    (format!(r"{rockstar}\GTAV"), "InstallFolderEpic"),
  ] {
    if let Some(dir) = registry::string(HKEY_LOCAL_MACHINE, &key, value) {
      let dir = PathBuf::from(dir);
      // Steam's value ends with a `GTAV` component that is not part of the install path.
      if let Some(parent) = dir.ends_with("GTAV").then(|| dir.parent()).flatten() {
        dirs.push(parent.to_path_buf());
      }
      dirs.push(dir);
    }
  }
  let steam = registry::string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam"));
  for library in steam_libraries(&steam) {
    dirs.push(library.join(r"steamapps\common\Grand Theft Auto V"));
  }
  dirs.extend(
    [
      r"C:\Program Files\Rockstar Games\Grand Theft Auto V",
      r"C:\Program Files\Epic Games\GTAV",
      r"C:\Program Files (x86)\Steam\steamapps\common\Grand Theft Auto V",
    ]
    .map(PathBuf::from),
  );
  dirs
}

#[cfg(not(windows))]
fn candidates() -> Vec<PathBuf> {
  let mut dirs = vec![PathBuf::from("/mnt/gtav")];
  if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
    for steam in [".steam/steam", ".local/share/Steam"] {
      for library in steam_libraries(&home.join(steam)) {
        dirs.push(library.join("steamapps/common/Grand Theft Auto V"));
      }
    }
  }
  dirs
}

/// Steam library roots listed in `steamapps/libraryfolders.vdf`, starting with `steam` itself.
fn steam_libraries(steam: &Path) -> Vec<PathBuf> {
  let mut libraries = vec![steam.to_path_buf()];
  let Ok(text) = std::fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) else {
    return libraries;
  };
  for line in text.lines() {
    let mut fields = line.split('"').filter(|field| !field.trim().is_empty());
    if let (Some("path"), Some(path)) = (fields.next(), fields.next()) {
      libraries.push(PathBuf::from(path.replace(r"\\", r"\")));
    }
  }
  libraries
}

#[cfg(windows)]
mod registry {
  use std::{ffi::OsString, os::windows::ffi::OsStringExt};

  use windows_sys::Win32::{
    Foundation::ERROR_SUCCESS,
    System::Registry::{HKEY, RRF_RT_REG_SZ, RegGetValueW},
  };

  /// Reads a `REG_SZ` value, or `None` when it is missing.
  pub fn string(
    root: HKEY,
    key: &str,
    value: &str,
  ) -> Option<String> {
    let key = wide(key);
    let value = wide(value);
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the key and value names are NUL-terminated, and `size` is the buffer size in bytes.
    let status = unsafe {
      RegGetValueW(
        root,
        key.as_ptr(),
        value.as_ptr(),
        RRF_RT_REG_SZ,
        std::ptr::null_mut(),
        buffer.as_mut_ptr().cast(),
        &mut size,
      )
    };
    if status != ERROR_SUCCESS {
      return None;
    }
    let length = (size as usize / 2).saturating_sub(1);
    let text = OsString::from_wide(&buffer[..length]).into_string().ok()?;
    let text = text.trim_end_matches('\0').trim().to_string();
    (!text.is_empty()).then_some(text)
  }

  fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn steam_libraries_reads_library_paths() {
    let root = std::env::temp_dir().join(format!("mlo_merger_steam_{}", std::process::id()));
    std::fs::create_dir_all(root.join("steamapps")).unwrap();
    std::fs::write(
      root.join("steamapps/libraryfolders.vdf"),
      "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t\t\"label\"\t\t\"\"\n\t}\n}\n",
    )
    .unwrap();
    assert_eq!(steam_libraries(&root), vec![root.clone(), PathBuf::from(r"D:\SteamLibrary")]);
    std::fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn status_requires_legacy_files() {
    let root = std::env::temp_dir().join(format!("mlo_merger_game_{}", std::process::id()));
    std::fs::create_dir_all(root.join("update")).unwrap();
    assert_eq!(status(&root), GameDirStatus::Unknown);
    std::fs::write(root.join("GTA5_Enhanced.exe"), b"").unwrap();
    assert_eq!(status(&root), GameDirStatus::Enhanced);
    std::fs::write(root.join("GTA5.exe"), b"").unwrap();
    std::fs::write(root.join("update/update.rpf"), b"").unwrap();
    assert_eq!(status(&root), GameDirStatus::Legacy);
    std::fs::remove_dir_all(root).unwrap();
  }
}
