//! In-process hosting of CodeWalker.Core (a managed .NET assembly) via CoreCLR,
//! used to convert `.ymap` <-> `.ymap.xml` without shelling out to a subprocess.
//!
//! CodeWalker.Core.dll itself (GPL-3.0) is never bundled with this repository;
//! callers must build `bridge/CodeWalker.Bridge` against a locally supplied copy
//! (see `CODEWALKER_CORE_DLL`) before this module can be used.

use std::ffi::CString;
use std::fmt;
use std::path::{Path, PathBuf};

use netcorehost::hostfxr::{
  AssemblyDelegateLoader, Hostfxr, HostfxrContext, InitializedForRuntimeConfig,
};
use netcorehost::nethost;
use netcorehost::pdcstring::PdCString;

type PreloadNamesFn = extern "system" fn(*const u8) -> i32;
type ConvertFn = extern "system" fn(*const u8, *const u8) -> i32;
type GetLastErrorFn = extern "system" fn(*mut u8, i32) -> i32;

#[derive(Debug)]
pub enum CodeWalkerError {
  Hosting(String),
  InvalidPath(String),
  Conversion {
    input: String,
    message: String,
  },
}

impl fmt::Display for CodeWalkerError {
  fn fmt(
    &self,
    f: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    match self {
      CodeWalkerError::Hosting(e) => write!(f, "failed to host CodeWalker.Bridge: {e}"),
      CodeWalkerError::InvalidPath(p) => write!(f, "path is not valid UTF-8: {p}"),
      CodeWalkerError::Conversion {
        input,
        message,
      } => {
        write!(f, "conversion of '{input}' failed: {message}")
      }
    }
  }
}

impl std::error::Error for CodeWalkerError {}

fn hosting<E: fmt::Debug>(e: E) -> CodeWalkerError {
  CodeWalkerError::Hosting(format!("{e:?}"))
}

// Well-known dotnet install locations, since the shell invoking this binary may not
// have DOTNET_ROOT/PATH set even though the SDK is installed (e.g. a fresh terminal).
fn resolve_dotnet_root() -> Option<PathBuf> {
  if let Ok(root) = std::env::var("DOTNET_ROOT") {
    return Some(PathBuf::from(root));
  }

  let dotnet_exe = if cfg!(windows) { "dotnet.exe" } else { "dotnet" };
  let home =
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);

  [
    home.map(|h| h.join(".dotnet")),
    Some(PathBuf::from("/usr/share/dotnet")),
    Some(PathBuf::from("/usr/lib/dotnet")),
    Some(PathBuf::from("/usr/local/share/dotnet")),
    Some(PathBuf::from("C:\\Program Files\\dotnet")),
  ]
  .into_iter()
  .flatten()
  .find(|root| root.join(dotnet_exe).exists())
}

/// A CoreCLR-hosted CodeWalker.Bridge instance. Not `Send`/`Sync`: the underlying
/// CodeWalker JenkIndex/JenkHash caches are process-global statics, so all calls
/// must come from the single thread that created this instance.
pub struct CodeWalker {
  // kept alive for the process lifetime; CoreCLR can only be initialized once per process
  _context: HostfxrContext<InitializedForRuntimeConfig>,
  _loader: AssemblyDelegateLoader,
  preload_names: PreloadNamesFn,
  ymap_to_xml: ConvertFn,
  xml_to_ymap: ConvertFn,
  get_last_error: GetLastErrorFn,
}

impl CodeWalker {
  /// Loads and initializes `bridge_dll` (built from `bridge/CodeWalker.Bridge`) in-process.
  pub fn init(bridge_dll: &Path) -> Result<Self, CodeWalkerError> {
    let runtime_config = bridge_dll.with_extension("runtimeconfig.json");

    let hostfxr: Hostfxr = match resolve_dotnet_root() {
      Some(root) => {
        nethost::load_hostfxr_with_dotnet_root(&path_to_pdcstring(&root)?).map_err(hosting)?
      }
      None => nethost::load_hostfxr().map_err(hosting)?,
    };
    let context = hostfxr
      .initialize_for_runtime_config(path_to_pdcstring(&runtime_config)?)
      .map_err(hosting)?;
    let loader =
      context.get_delegate_loader_for_assembly(path_to_pdcstring(bridge_dll)?).map_err(hosting)?;

    let type_name =
      PdCString::from_os_str(std::ffi::OsStr::new("CodeWalker.Bridge.Exports, CodeWalker.Bridge"))
        .map_err(|_| CodeWalkerError::InvalidPath("CodeWalker.Bridge.Exports".into()))?;

    let preload_names = *loader
      .get_function_with_unmanaged_callers_only::<PreloadNamesFn>(
        &type_name,
        &pdcstr("PreloadNames")?,
      )
      .map_err(hosting)?;
    let ymap_to_xml = *loader
      .get_function_with_unmanaged_callers_only::<ConvertFn>(&type_name, &pdcstr("YmapToXml")?)
      .map_err(hosting)?;
    let xml_to_ymap = *loader
      .get_function_with_unmanaged_callers_only::<ConvertFn>(&type_name, &pdcstr("XmlToYmap")?)
      .map_err(hosting)?;
    let get_last_error = *loader
      .get_function_with_unmanaged_callers_only::<GetLastErrorFn>(
        &type_name,
        &pdcstr("GetLastError")?,
      )
      .map_err(hosting)?;

    Ok(Self {
      _context: context,
      _loader: loader,
      preload_names,
      ymap_to_xml,
      xml_to_ymap,
      get_last_error,
    })
  }

  /// Loads `input` (a `.ymap` file) and discards it, registering its embedded
  /// string names into CodeWalker's global JenkIndex so that later exports can
  /// resolve hash references by name instead of falling back to `hash_XXXXXXXX`.
  pub fn preload_names(
    &self,
    input: &Path,
  ) -> Result<(), CodeWalkerError> {
    let input_c = path_to_cstring(input)?;
    let code = (self.preload_names)(input_c.as_ptr() as *const u8);
    self.check(code, input)
  }

  /// Converts a binary `.ymap` file at `input` into a `.ymap.xml` file at `output`.
  pub fn ymap_to_xml(
    &self,
    input: &Path,
    output: &Path,
  ) -> Result<(), CodeWalkerError> {
    self.convert(self.ymap_to_xml, input, output)
  }

  /// Converts a `.ymap.xml` file at `input` back into a binary `.ymap` file at `output`.
  pub fn xml_to_ymap(
    &self,
    input: &Path,
    output: &Path,
  ) -> Result<(), CodeWalkerError> {
    self.convert(self.xml_to_ymap, input, output)
  }

  fn convert(
    &self,
    f: ConvertFn,
    input: &Path,
    output: &Path,
  ) -> Result<(), CodeWalkerError> {
    let input_c = path_to_cstring(input)?;
    let output_c = path_to_cstring(output)?;
    let code = f(input_c.as_ptr() as *const u8, output_c.as_ptr() as *const u8);
    self.check(code, input)
  }

  fn check(
    &self,
    code: i32,
    input: &Path,
  ) -> Result<(), CodeWalkerError> {
    if code == 0 {
      return Ok(());
    }
    let mut buf = vec![0u8; 8192];
    let len = (self.get_last_error)(buf.as_mut_ptr(), buf.len() as i32);
    buf.truncate(len.max(0) as usize);
    Err(CodeWalkerError::Conversion {
      input: input.display().to_string(),
      message: String::from_utf8_lossy(&buf).into_owned(),
    })
  }
}

fn path_to_cstring(p: &Path) -> Result<CString, CodeWalkerError> {
  let s = p.to_str().ok_or_else(|| CodeWalkerError::InvalidPath(p.display().to_string()))?;
  CString::new(s).map_err(|_| CodeWalkerError::InvalidPath(p.display().to_string()))
}

fn path_to_pdcstring(p: &Path) -> Result<PdCString, CodeWalkerError> {
  PdCString::from_os_str(p.as_os_str())
    .map_err(|_| CodeWalkerError::InvalidPath(p.display().to_string()))
}

fn pdcstr(s: &str) -> Result<PdCString, CodeWalkerError> {
  PdCString::from_os_str(std::ffi::OsStr::new(s))
    .map_err(|_| CodeWalkerError::InvalidPath(s.to_string()))
}
