#[derive(struct_diff::Diff)]
struct Settings {
  #[diff(context = 1.0)]
  speed: f32,
}

fn main() {}
