//! Local real-resource regression tests. Fixtures and DLLs are not redistributed.

mod common;
mod compare;
mod extract;
mod report;

macro_rules! sample_case {
  ($case:ident, $file:literal) => {
    mod $case {
      #[test]
      #[ignore = "requires local asset/sample fixtures and CodeWalker.Core.dll"]
      fn to_xml() {
        crate::common::check(super::EXTENSION, $file, false);
      }

      #[test]
      #[ignore = "requires local asset/sample fixtures and CodeWalker.Core.dll"]
      fn from_xml() {
        crate::common::check(super::EXTENSION, $file, true);
      }
    }
  };
}

mod ybn;
mod ymap;
mod ymt;
mod ynd;
mod ytyp;
