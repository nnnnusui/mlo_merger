//! Window text in each supported language.
//!
//! Entries with `{}` take one argument through [`fill`].

/// A language the window can be shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
  #[default]
  En,
  Ja,
}

impl Lang {
  pub const ALL: [Lang; 2] = [Lang::En, Lang::Ja];

  /// The language's name in that language, for the selector.
  pub fn name(self) -> &'static str {
    match self {
      Lang::En => "English",
      Lang::Ja => "日本語",
    }
  }

  pub fn text(self) -> &'static Text {
    match self {
      Lang::En => &EN,
      Lang::Ja => &JA,
    }
  }
}

/// Replaces the `{}` in `template` with `value`.
pub fn fill(
  template: &str,
  value: impl std::fmt::Display,
) -> String {
  template.replacen("{}", &value.to_string(), 1)
}

/// Every string the window shows.
pub struct Text {
  pub heading: &'static str,
  pub intro: &'static str,
  pub browse: &'static str,

  pub game_dir_label: &'static str,
  pub game_dir_dialog: &'static str,
  pub game_dir_empty: &'static str,
  pub game_dir_ok: &'static str,
  pub game_dir_enhanced: &'static str,
  pub game_dir_unknown: &'static str,

  pub magic_label: &'static str,
  pub magic_dialog: &'static str,
  pub magic_ok: &'static str,
  pub magic_missing: &'static str,
  pub magic_download: &'static str,
  /// `{}`: destination path.
  pub magic_download_hover: &'static str,
  pub magic_page: &'static str,
  pub magic_downloaded: &'static str,
  /// `{}`: error.
  pub magic_download_failed: &'static str,

  pub generate_label: &'static str,
  /// `{}`: output directory.
  pub output: &'static str,
  pub output_replaced: &'static str,
  pub generate: &'static str,
  pub generating: &'static str,
  pub generating_note: &'static str,
  pub generated: &'static str,
  pub open_vanilla: &'static str,
  /// `{}`: error.
  pub generate_failed: &'static str,
  /// `{}`: log file path.
  pub log_file: &'static str,
  pub open_log_dir: &'static str,

  pub log: &'static str,
}

const EN: Text = Text {
  heading: "Generate vanilla data",
  intro: "Creates the vanilla folder from your GTA V game data. \
          Upload the generated folder to the server.",
  browse: "Browse…",

  game_dir_label: "1. GTA V (Legacy) install folder",
  game_dir_dialog: "Select the GTA V install folder",
  game_dir_empty: "Select the folder that contains GTA5.exe",
  game_dir_ok: "Found GTA5.exe",
  game_dir_enhanced: "The Enhanced edition is not supported. Select a Legacy install.",
  game_dir_unknown: "GTA5.exe and update/update.rpf were not found",

  magic_label: "2. magic.dat (a CodeWalker file)",
  magic_dialog: "Select magic.dat",
  magic_ok: "Found magic.dat",
  magic_missing: "magic.dat is missing",
  magic_download: "Download from GitHub",
  magic_download_hover: "Saves to {}",
  magic_page: "Open the GitHub page",
  magic_downloaded: "Downloaded",
  magic_download_failed: "Download failed: {}",

  generate_label: "3. Generate",
  output: "Output: {}",
  output_replaced: "The existing vanilla folder will be replaced",
  generate: "Generate vanilla",
  generating: "Generating…",
  generating_note: "This can take a few minutes",
  generated: "Done. Upload the vanilla folder to the server.",
  open_vanilla: "Open the vanilla folder",
  generate_failed: "Generation failed: {}",
  log_file: "Log: {}",
  open_log_dir: "Open the log folder",

  log: "Log",
};

const JA: Text = Text {
  heading: "vanilla データの生成",
  intro: "GTA V のゲームデータから vanilla フォルダを作ります。\
          生成したフォルダをサーバーにアップロードしてください。",
  browse: "参照…",

  game_dir_label: "1. GTA V (Legacy) のインストールフォルダ",
  game_dir_dialog: "GTA V のインストールフォルダを選択",
  game_dir_empty: "GTA5.exe があるフォルダを選択してください",
  game_dir_ok: "GTA5.exe を確認しました",
  game_dir_enhanced: "Enhanced 版は非対応です。Legacy 版のフォルダを選択してください",
  game_dir_unknown: "GTA5.exe と update/update.rpf が見つかりません",

  magic_label: "2. magic.dat（CodeWalker のファイル）",
  magic_dialog: "magic.dat を選択",
  magic_ok: "magic.dat を確認しました",
  magic_missing: "magic.dat がありません",
  magic_download: "GitHub からダウンロード",
  magic_download_hover: "{} に保存します",
  magic_page: "GitHub のページを開く",
  magic_downloaded: "ダウンロードしました",
  magic_download_failed: "ダウンロードに失敗しました: {}",

  generate_label: "3. 生成",
  output: "出力先: {}",
  output_replaced: "既存の vanilla フォルダは置き換えられます",
  generate: "vanilla を生成",
  generating: "生成中…",
  generating_note: "数分かかることがあります",
  generated: "完了しました。vanilla フォルダをサーバーにアップロードしてください。",
  open_vanilla: "vanilla フォルダを開く",
  generate_failed: "生成に失敗しました: {}",
  log_file: "ログ: {}",
  open_log_dir: "ログフォルダを開く",

  log: "ログ",
};
