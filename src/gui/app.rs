//! The vanilla generator window.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};

use mlo_merger::core::vanilla::GenerateVanilla;

use super::{
  game_dir::{self, GameDirStatus},
  i18n::{Lang, Text, fill},
  log_view::LogView,
  magic,
  task::Task,
};

const OK: Color32 = Color32::from_rgb(0x3c, 0xa0, 0x5a);
const WARN: Color32 = Color32::from_rgb(0xd0, 0x8c, 0x1c);
const ERROR: Color32 = Color32::from_rgb(0xd0, 0x45, 0x45);

/// What the running [`Task`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobKind {
  Generate,
  DownloadMagic,
}

/// The last finished job, shown until the next one starts.
struct Outcome {
  kind: JobKind,
  result: Result<(), String>,
}

pub struct VanillaApp {
  /// The executable's directory.
  base_dir: PathBuf,
  log_path: PathBuf,
  log_view: LogView,
  game_dir: String,
  magic: String,
  job: Option<(JobKind, Task)>,
  outcome: Option<Outcome>,
  lang: Lang,
}

impl VanillaApp {
  pub fn new(
    base_dir: PathBuf,
    log_path: PathBuf,
    log_view: LogView,
  ) -> Self {
    let game_dir = game_dir::detect().map(|dir| dir.display().to_string()).unwrap_or_default();
    let magic = base_dir.join("magic.dat").display().to_string();
    Self {
      base_dir,
      log_path,
      log_view,
      game_dir,
      magic,
      job: None,
      outcome: None,
      lang: Lang::default(),
    }
  }

  fn output_dir(&self) -> PathBuf {
    self.base_dir.join("vanilla")
  }

  fn magic_download_path(&self) -> PathBuf {
    self.base_dir.join("magic.dat")
  }

  fn text(&self) -> &'static Text {
    self.lang.text()
  }

  fn running(&self) -> Option<JobKind> {
    self.job.as_ref().map(|(kind, _)| *kind)
  }

  fn start(
    &mut self,
    context: &egui::Context,
    kind: JobKind,
  ) {
    self.outcome = None;
    let task = match kind {
      JobKind::Generate => {
        self.log_view.clear();
        let command = GenerateVanilla {
          game_dir: PathBuf::from(self.game_dir.trim()),
          output_dir: self.output_dir(),
          magic: PathBuf::from(self.magic.trim()),
          reuse_existing: false,
        };
        Task::spawn(context, move || {
          tracing::info!("Generating {}", command.output_dir.display());
          command.run().map_err(|error| error.to_string())?;
          tracing::info!("Generated {}", command.output_dir.display());
          Ok::<(), String>(())
        })
      }
      JobKind::DownloadMagic => {
        let destination = self.magic_download_path();
        Task::spawn(context, move || magic::download(&destination))
      }
    };
    self.job = Some((kind, task));
  }

  fn poll_job(&mut self) {
    let Some((kind, task)) = &self.job else {
      return;
    };
    let Some(result) = task.poll() else {
      return;
    };
    let kind = *kind;
    if kind == JobKind::DownloadMagic && result.is_ok() {
      self.magic = self.magic_download_path().display().to_string();
    }
    self.outcome = Some(Outcome {
      kind,
      result,
    });
    self.job = None;
  }

  fn game_dir_row(
    &mut self,
    ui: &mut egui::Ui,
  ) -> bool {
    let text = self.text();
    ui.label(RichText::new(text.game_dir_label).strong());
    ui.horizontal(|ui| {
      if ui.button(text.browse).clicked() {
        let mut dialog = rfd::FileDialog::new().set_title(text.game_dir_dialog);
        if Path::new(self.game_dir.trim()).is_dir() {
          dialog = dialog.set_directory(self.game_dir.trim());
        }
        if let Some(dir) = dialog.pick_folder() {
          self.game_dir = dir.display().to_string();
        }
      }
      ui.add(egui::TextEdit::singleline(&mut self.game_dir).desired_width(f32::INFINITY));
    });
    let path = self.game_dir.trim();
    let (ready, message, color) = if path.is_empty() {
      (false, text.game_dir_empty, WARN)
    } else {
      match game_dir::status(Path::new(path)) {
        GameDirStatus::Legacy => (true, text.game_dir_ok, OK),
        GameDirStatus::Enhanced => (false, text.game_dir_enhanced, ERROR),
        GameDirStatus::Unknown => (false, text.game_dir_unknown, ERROR),
      }
    };
    ui.label(RichText::new(message).color(color));
    ready
  }

  fn magic_row(
    &mut self,
    ui: &mut egui::Ui,
  ) -> bool {
    let text = self.text();
    ui.label(RichText::new(text.magic_label).strong());
    ui.add_enabled_ui(self.job.is_none(), |ui| {
      ui.horizontal(|ui| {
        if ui.button(text.browse).clicked()
          && let Some(file) = rfd::FileDialog::new()
            .set_title(text.magic_dialog)
            .add_filter("magic.dat", &["dat"])
            .set_directory(&self.base_dir)
            .pick_file()
        {
          self.magic = file.display().to_string();
        }
        ui.add(egui::TextEdit::singleline(&mut self.magic).desired_width(f32::INFINITY));
      });
    });
    let ready = Path::new(self.magic.trim()).is_file();
    ui.horizontal(|ui| {
      if ready {
        ui.label(RichText::new(text.magic_ok).color(OK));
      } else {
        ui.label(RichText::new(text.magic_missing).color(ERROR));
      }
      ui.separator();
      let downloading = self.running() == Some(JobKind::DownloadMagic);
      let button = egui::Button::new(text.magic_download);
      if ui
        .add_enabled(self.job.is_none(), button)
        .on_hover_text(fill(text.magic_download_hover, self.magic_download_path().display()))
        .clicked()
      {
        self.start(ui.ctx(), JobKind::DownloadMagic);
      }
      if downloading {
        ui.spinner();
      }
      ui.hyperlink_to(text.magic_page, magic::PAGE_URL);
    });
    if let Some(Outcome {
      kind: JobKind::DownloadMagic,
      result,
    }) = &self.outcome
    {
      match result {
        Ok(()) => ui.label(RichText::new(text.magic_downloaded).color(OK)),
        Err(error) => ui.label(RichText::new(fill(text.magic_download_failed, error)).color(ERROR)),
      };
    }
    ready
  }

  fn generate_row(
    &mut self,
    ui: &mut egui::Ui,
    ready: bool,
  ) {
    let text = self.text();
    ui.label(RichText::new(text.generate_label).strong());
    let output = self.output_dir();
    ui.label(fill(text.output, output.display()));
    if output.exists() {
      ui.label(RichText::new(text.output_replaced).color(WARN));
    }
    ui.horizontal(|ui| {
      let generating = self.running() == Some(JobKind::Generate);
      let label = if generating { text.generating } else { text.generate };
      let button =
        egui::Button::new(RichText::new(label).size(18.0)).min_size(egui::vec2(180.0, 36.0));
      if ui.add_enabled(ready && self.job.is_none(), button).clicked() {
        self.start(ui.ctx(), JobKind::Generate);
      }
      if generating {
        ui.spinner();
        ui.label(text.generating_note);
      }
    });

    if let Some(Outcome {
      kind: JobKind::Generate,
      result,
    }) = &self.outcome
    {
      ui.add_space(4.0);
      match result {
        Ok(()) => {
          ui.label(RichText::new(text.generated).color(OK).strong());
          if ui.button(text.open_vanilla).clicked() {
            open_in_file_manager(&output);
          }
        }
        Err(error) => {
          ui.label(RichText::new(fill(text.generate_failed, error)).color(ERROR).strong());
          ui.horizontal(|ui| {
            ui.label(fill(text.log_file, self.log_path.display()));
            if ui.button(text.open_log_dir).clicked()
              && let Some(dir) = self.log_path.parent()
            {
              open_in_file_manager(dir);
            }
          });
        }
      }
    }
  }

  fn language_selector(
    &mut self,
    ui: &mut egui::Ui,
  ) {
    egui::ComboBox::from_id_salt("language").selected_text(self.lang.name()).show_ui(ui, |ui| {
      for lang in Lang::ALL {
        ui.selectable_value(&mut self.lang, lang, lang.name());
      }
    });
  }

  fn log_panel(
    &self,
    ui: &mut egui::Ui,
  ) {
    egui::CollapsingHeader::new(self.text().log).id_salt("log").default_open(true).show(ui, |ui| {
      egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(
        ui,
        |ui| {
          self.log_view.with_lines(|lines| {
            for line in lines {
              ui.label(RichText::new(line).monospace().size(12.0));
            }
          });
        },
      );
    });
  }
}

impl eframe::App for VanillaApp {
  fn ui(
    &mut self,
    ui: &mut egui::Ui,
    _frame: &mut eframe::Frame,
  ) {
    self.poll_job();
    egui::Frame::central_panel(ui.style()).show(ui, |ui| {
      ui.horizontal(|ui| {
        ui.heading(self.text().heading);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          self.language_selector(ui);
        });
      });
      ui.label(self.text().intro);
      ui.separator();
      let enabled = self.job.is_none();
      let game_ready = ui.add_enabled_ui(enabled, |ui| self.game_dir_row(ui)).inner;
      ui.add_space(8.0);
      let magic_ready = self.magic_row(ui);
      ui.add_space(8.0);
      self.generate_row(ui, game_ready && magic_ready);
      ui.separator();
      self.log_panel(ui);
    });
  }
}

/// Shows `path` in the platform's file manager.
fn open_in_file_manager(path: &Path) {
  let program = if cfg!(windows) {
    "explorer"
  } else if cfg!(target_os = "macos") {
    "open"
  } else {
    "xdg-open"
  };
  if let Err(error) = std::process::Command::new(program).arg(path).spawn() {
    tracing::warn!("Could not open {}: {error}", path.display());
  }
}
