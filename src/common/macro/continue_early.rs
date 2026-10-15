#[macro_export]
macro_rules! continue_early {
  (if $cond:tt continue) => {
    #[allow(unused_parens)]
    if $cond {
      continue;
    }
  };
}
