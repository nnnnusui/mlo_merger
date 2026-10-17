#[derive(Debug, Clone, PartialEq)]
struct Plain {
  value: u32,
}

#[derive(struct_diff::Diff)]
struct Holder {
  #[diff(nested)]
  plain: Plain,
}

fn main() {}
