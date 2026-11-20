//! Prints the lines of the source files git tracks in the repository, by language.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// The source languages counted, by file extension.
const LANGUAGES: &[(&str, &str)] = &[
  ("rs", "Rust"),
  ("js", "JavaScript"),
  ("mjs", "JavaScript"),
  ("ts", "TypeScript"),
  ("py", "Python"),
  ("sh", "Shell"),
  ("html", "HTML"),
];

#[derive(Default)]
struct Count {
  files: usize,
  lines: usize,
  non_blank: usize,
}

impl Count {
  fn add(
    &mut self,
    other: &Count,
  ) {
    self.files += other.files;
    self.lines += other.lines;
    self.non_blank += other.non_blank;
  }
}

fn main() {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
  let output =
    Command::new("git").arg("ls-files").current_dir(&root).output().expect("git ls-files runs");
  assert!(
    output.status.success(),
    "git ls-files fails: {}",
    String::from_utf8_lossy(&output.stderr)
  );

  let mut counts = BTreeMap::<&str, Count>::new();
  for path in String::from_utf8_lossy(&output.stdout).lines() {
    let extension = Path::new(path).extension().and_then(|extension| extension.to_str());
    let Some(&(_, language)) = LANGUAGES.iter().find(|(known, _)| Some(*known) == extension) else {
      continue;
    };
    // Files deleted from the working tree but still tracked are skipped.
    let Ok(text) = std::fs::read_to_string(root.join(path)) else {
      continue;
    };
    let count = counts.entry(language).or_default();
    count.files += 1;
    count.lines += text.lines().count();
    count.non_blank += text.lines().filter(|line| !line.trim().is_empty()).count();
  }

  let mut total = Count::default();
  println!("{:<12} {:>7} {:>9} {:>11}", "Language", "Files", "Lines", "Non-blank");
  for (language, count) in &counts {
    println!("{language:<12} {:>7} {:>9} {:>11}", count.files, count.lines, count.non_blank);
    total.add(count);
  }
  println!("{:<12} {:>7} {:>9} {:>11}", "Total", total.files, total.lines, total.non_blank);
}
