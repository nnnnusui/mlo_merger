#[derive(struct_diff::Diff)]
struct Settings {
  #[diff(nested, threshold = 1.0)]
  speed: f32,
}

fn main() {}
