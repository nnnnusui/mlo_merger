#[derive(struct_diff::Diff)]
struct Settings {
  #[diff(by_key = id)]
  items: Vec<u32>,
}

fn main() {}
