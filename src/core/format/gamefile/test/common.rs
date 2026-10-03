use std::{
  fs,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  sync::{OnceLock, mpsc},
  time::{Duration, Instant},
};

use mlo_merger::core::{
  codewalker::CodeWalker,
  format::gamefile::{
    meta_resource::{MetaResource, MetaSchemaCatalog},
    resource_convert::{NativeResourceFormat, resource_to_xml, xml_to_resource},
    resource_file::Rsc7Resource,
  },
};
use serde::Serialize;

use super::compare::{assert_canonical_xml_eq, assert_ybn_xml_eq, ybn_resource_quantums};

struct Request {
  extension: &'static str,
  file: &'static str,
  rebuild: bool,
  response: mpsc::Sender<Result<(), String>>,
}

#[derive(Default, Serialize)]
struct Report {
  fixture: String,
  direction: String,
  native_binary_identical_to_source: Option<bool>,
  codewalker_binary_identical_to_source: Option<bool>,
  native_rebuild_preserves_source_xml: Option<bool>,
  codewalker_rebuild_preserves_source_xml: Option<bool>,
  error: Option<String>,
}

pub(super) fn check(
  extension: &'static str,
  file: &'static str,
  rebuild: bool,
) {
  if rebuild {
    if let Err(error) = isolated_rebuild(extension, file) {
      panic!("{extension}/{file}: {error}");
    }
    return;
  }
  static WORKER: OnceLock<mpsc::Sender<Request>> = OnceLock::new();
  let worker = WORKER.get_or_init(|| {
    let (sender, receiver) = mpsc::channel::<Request>();
    std::thread::spawn(move || {
      let base = Path::new(env!("CARGO_MANIFEST_DIR"));
      let bridge = std::env::var_os("CODEWALKER_BRIDGE_DLL")
        .map(PathBuf::from)
        .unwrap_or_else(|| base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll"));
      let codewalker = CodeWalker::init(&bridge).map_err(|error| error.to_string());
      for request in receiver {
        let result = match &codewalker {
          Ok(codewalker) => run_case(base, codewalker, &request),
          Err(error) => Err(error.clone()),
        };
        let _ = request.response.send(result);
      }
    });
    sender
  });
  let (response, receiver) = mpsc::channel();
  worker
    .send(Request {
      extension,
      file,
      rebuild,
      response,
    })
    .expect("DLL worker stopped");
  if let Err(error) = receiver.recv().expect("DLL worker did not return a result") {
    panic!("{extension}/{file}: {error}");
  }
}

fn isolated_rebuild(
  extension: &str,
  file: &str,
) -> Result<(), String> {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let output = base.join("asset/sample/reports").join(extension);
  fs::create_dir_all(&output).map_err(|error| error.to_string())?;
  let report_path = output.join(format!("{file}.from_xml.json"));
  if report_path.exists() {
    fs::remove_file(&report_path).map_err(|error| error.to_string())?;
  }
  let limit = std::env::var("SAMPLE_TIMEOUT_SECONDS")
    .ok()
    .map(|value| value.parse::<u64>().map_err(|error| error.to_string()))
    .transpose()?
    .unwrap_or(60);
  let mut child = Command::new(std::env::current_exe().map_err(|error| error.to_string())?)
    .args(["--exact", "common::rebuild_worker", "--ignored"])
    .env("GAMEFILE_SAMPLE_EXTENSION", extension)
    .env("GAMEFILE_SAMPLE_FILE", file)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|error| error.to_string())?;
  let start = Instant::now();
  let error = loop {
    match child.try_wait() {
      Ok(Some(status)) => {
        if status.success() {
          return Ok(());
        }
        if let Ok(bytes) = fs::read(&report_path)
          && let Ok(report) = serde_json::from_slice::<serde_json::Value>(&bytes)
          && let Some(error) = report["error"].as_str()
        {
          return Err(error.to_string());
        }
        break format!("isolated rebuild exited with {status} before reporting a result");
      }
      Ok(None) if start.elapsed() < Duration::from_secs(limit) => {
        std::thread::sleep(Duration::from_millis(20));
      }
      Ok(None) => break format!("isolated rebuild exceeded {limit} seconds"),
      Err(error) => break format!("cannot wait for isolated rebuild: {error}"),
    }
  };
  let _ = child.kill();
  let _ = child.wait();
  let report = Report {
    fixture: format!("{extension}/{file}"),
    direction: "from_xml".into(),
    error: Some(error.clone()),
    ..Report::default()
  };
  fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap())
    .map_err(|error| error.to_string())?;
  let _ =
    fs::remove_dir_all(std::env::temp_dir().join(format!("mlo_merger_samples_{}", child.id())));
  Err(error)
}

#[test]
#[ignore = "internal worker invoked by an individual from_xml test"]
fn rebuild_worker() {
  let Ok(extension) = std::env::var("GAMEFILE_SAMPLE_EXTENSION") else {
    return;
  };
  let file = std::env::var("GAMEFILE_SAMPLE_FILE").expect("worker fixture missing");
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge = std::env::var_os("CODEWALKER_BRIDGE_DLL")
    .map(PathBuf::from)
    .unwrap_or_else(|| base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll"));
  let codewalker = CodeWalker::init(&bridge).expect("cannot initialize CodeWalker worker");
  let (response, _) = mpsc::channel();
  let request = Request {
    extension: Box::leak(extension.into_boxed_str()),
    file: Box::leak(file.into_boxed_str()),
    rebuild: true,
    response,
  };
  if let Err(error) = run_case(base, &codewalker, &request) {
    panic!("{error}");
  }
}

fn run_case(
  base: &Path,
  codewalker: &CodeWalker,
  request: &Request,
) -> Result<(), String> {
  let direction = if request.rebuild { "from_xml" } else { "to_xml" };
  let fixture = format!("{}/{}", request.extension, request.file);
  let output = base.join("asset/sample/reports").join(request.extension);
  fs::create_dir_all(&output).map_err(|error| error.to_string())?;
  let temporary = std::env::temp_dir().join(format!("mlo_merger_samples_{}", std::process::id()));
  fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
  let mut report = Report {
    fixture,
    direction: direction.into(),
    ..Report::default()
  };
  let result = compare_case(base, codewalker, request, &temporary, &mut report);
  report.error = result.as_ref().err().cloned();
  fs::write(
    output.join(format!("{}.{}.json", request.file, direction)),
    serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
  )
  .map_err(|error| error.to_string())?;
  fs::remove_dir_all(temporary).map_err(|error| error.to_string())?;
  result
}

fn compare_case(
  base: &Path,
  codewalker: &CodeWalker,
  request: &Request,
  temporary: &Path,
  report: &mut Report,
) -> Result<(), String> {
  let input = base.join("asset/sample").join(request.extension).join(request.file);
  let source = fs::read(&input).map_err(|error| error.to_string())?;
  let format = NativeResourceFormat::from_path(&input).map_err(|error| error.to_string())?;
  let mut catalog = MetaSchemaCatalog::default();
  if matches!(
    format,
    NativeResourceFormat::Ymap | NativeResourceFormat::Ytyp | NativeResourceFormat::YmtRsc
  ) && let Ok(resource) = Rsc7Resource::decode(&source)
    && let Ok(meta) = MetaResource::parse(&resource)
  {
    catalog.add_resource(&meta);
  }
  let reference_path = temporary.join(format!("{}.xml", request.file));
  export(codewalker, format, &input, &reference_path)?;
  let reference = fs::read_to_string(&reference_path).map_err(|error| error.to_string())?;
  if reference.contains("<error") {
    return Err(
      "CodeWalker export contains an error node (missing schema or invalid source)".into(),
    );
  }
  if !request.rebuild {
    let native = resource_to_xml(format, &source, &catalog.hash_names)
      .map_err(|error| format!("Native to-xml: {error}"))?;
    return assert_canonical_xml_eq(
      &native,
      &reference,
      &format!("Native export {}", input.display()),
    );
  }

  let native = xml_to_resource(format, &reference, &catalog)
    .map_err(|error| format!("Native from-xml: {error}"))?;
  report.native_binary_identical_to_source = Some(native == source);
  let native_path = temporary.join(request.file);
  fs::write(&native_path, &native).map_err(|error| error.to_string())?;
  let native_xml_path = temporary.join("native.xml");
  export(codewalker, format, &native_path, &native_xml_path)?;
  let native_xml = fs::read_to_string(native_xml_path).map_err(|error| error.to_string())?;
  report.native_rebuild_preserves_source_xml =
    Some(assert_canonical_xml_eq(&native_xml, &reference, "Native source round-trip").is_ok());

  let reference_binary_path = temporary.join("codewalker").join(request.file);
  fs::create_dir_all(reference_binary_path.parent().unwrap()).map_err(|error| error.to_string())?;
  if format == NativeResourceFormat::Ymap {
    codewalker.xml_to_ymap(&reference_path, &reference_binary_path)
  } else {
    codewalker.game_file_from_xml(&reference_path, &reference_binary_path)
  }
  .map_err(|error| format!("CodeWalker from-xml: {error}"))?;
  let reference_binary = fs::read(&reference_binary_path).map_err(|error| error.to_string())?;
  report.codewalker_binary_identical_to_source = Some(reference_binary == source);
  let rebuilt_reference_path = temporary.join("reference.xml");
  export(codewalker, format, &reference_binary_path, &rebuilt_reference_path)?;
  let rebuilt_reference =
    fs::read_to_string(rebuilt_reference_path).map_err(|error| error.to_string())?;
  report.codewalker_rebuild_preserves_source_xml =
    Some(assert_canonical_xml_eq(&rebuilt_reference, &reference, "CodeWalker round-trip").is_ok());
  if native_xml.contains("<error") || rebuilt_reference.contains("<error") {
    return Err("rebuilt binary exports an error node".into());
  }
  if format == NativeResourceFormat::Ybn {
    let native_quantums = ybn_resource_quantums(&native)?;
    let reference_quantums = ybn_resource_quantums(&reference_binary)?;
    assert_ybn_xml_eq(
      &native_xml,
      &rebuilt_reference,
      "Native from-xml",
      true,
      Some((&native_quantums, &reference_quantums)),
    )
  } else {
    assert_canonical_xml_eq(&native_xml, &rebuilt_reference, "Native from-xml")
  }
}

fn export(
  codewalker: &CodeWalker,
  format: NativeResourceFormat,
  input: &Path,
  output: &Path,
) -> Result<(), String> {
  if format == NativeResourceFormat::Ymap {
    codewalker.ymap_to_xml(input, output)
  } else {
    codewalker.game_file_to_xml(input, output)
  }
  .map_err(|error| format!("CodeWalker to-xml: {error}"))
}
