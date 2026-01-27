#[macro_export]
macro_rules! return_early {
  (if $cond:tt return $ret:expr) => {
    #[allow(unused_parens)]
    if $cond {
      return $ret;
    }
  };
}
