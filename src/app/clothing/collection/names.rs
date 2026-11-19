//! Addon clothing stream file names.
//!
//! A collection's files are named `{prefix}^{body}.{extension}`. The prefix is
//! `{ped}_{collection}` for components and `{ped}_p_{collection}` for props; the body names the
//! component or prop anchor, the drawable index and what the file holds:
//!
//! | File | Body |
//! | --- | --- |
//! | Component drawable | `jbib_000_u.ydd`, `jbib_000_u_1.ydd` |
//! | Component texture | `jbib_diff_000_a_uni.ytd` |
//! | Component cloth | `jbib_000_u.yld` |
//! | Prop drawable | `p_head_000.ydd` |
//! | Prop texture | `p_head_diff_000_a.ytd` |
//!
//! Loose files have the body alone, without a prefix.

/// The component slots, as their file names spell them, in `availComp` order.
pub const COMPONENT_NAMES: [&str; 12] =
  ["head", "berd", "hair", "uppr", "lowr", "hand", "feet", "teef", "accs", "task", "decl", "jbib"];

/// What part of a collection a file belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
  /// A component, by its index in [`COMPONENT_NAMES`].
  Component(usize),
  /// A prop anchor, as the file name spells it after `p_`, such as `head` or `lwrist`.
  Prop(String),
}

impl Slot {
  /// The slot as file names spell it: `jbib`, or `p_head`.
  pub fn file_name(&self) -> String {
    match self {
      Self::Component(index) => COMPONENT_NAMES[*index].to_owned(),
      Self::Prop(anchor) => format!("p_{anchor}"),
    }
  }
}

/// What a file holds for its drawable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileRole {
  /// The drawable model (`.ydd`).
  Drawable,
  /// One of its textures (`_diff_`, `.ytd`).
  Texture,
  /// Its cloth simulation (`.yld`).
  Cloth,
}

/// A parsed addon clothing stream file name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AddonFileName {
  /// The lowercase prefix before `^`, naming the Ped model and collection; none for a loose file.
  pub prefix: Option<String>,
  /// The component or prop anchor.
  pub slot: Slot,
  /// What the file holds.
  pub role: FileRole,
  /// The drawable or prop index within its slot.
  pub index: u32,
  /// The lowercase rest of the body after the index, with its leading `_`, such as `_u_1` or
  /// `_a_uni`; empty when the body ends at the index.
  pub tail: String,
}

impl AddonFileName {
  /// Parses `file_name`, matched case-insensitively; none when it is no addon clothing file.
  pub fn parse(file_name: &str) -> Option<Self> {
    let lower = file_name.to_ascii_lowercase();
    let (stem, extension) = lower.rsplit_once('.')?;
    let role = match extension {
      "ydd" => FileRole::Drawable,
      "ytd" => FileRole::Texture,
      "yld" => FileRole::Cloth,
      _ => return None,
    };
    let (prefix, body) = match stem.split_once('^') {
      Some((prefix, body)) if !prefix.is_empty() && !body.contains('^') => {
        (Some(prefix.to_owned()), body)
      }
      Some(_) => return None,
      None => (None, stem),
    };
    let (slot, rest) = parse_slot(body)?;
    let rest = match role {
      FileRole::Texture => rest.strip_prefix("_diff")?,
      FileRole::Drawable | FileRole::Cloth => rest,
    };
    let rest = rest.strip_prefix('_')?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits < 3 {
      return None;
    }
    let index = rest[..digits].parse().ok()?;
    let tail = &rest[digits..];
    if !(tail.is_empty() || tail.starts_with('_') && tail.len() > 1) {
      return None;
    }
    // A component drawable or cloth names its variant (`_u`, `_r`), and a texture its letter.
    if tail.is_empty() && (role == FileRole::Texture || matches!(slot, Slot::Component(_))) {
      return None;
    }
    Some(Self {
      prefix,
      slot,
      role,
      index,
      tail: tail.to_owned(),
    })
  }

  /// The file name with `prefix` (none for a loose file) and drawable `index` in place of this
  /// one's, everything else kept.
  pub fn renamed(
    &self,
    prefix: Option<&str>,
    index: u32,
  ) -> String {
    let prefix = prefix.map_or(String::new(), |prefix| format!("{prefix}^"));
    let slot = self.slot.file_name();
    let (diff, extension) = match self.role {
      FileRole::Drawable => ("", "ydd"),
      FileRole::Texture => ("_diff", "ytd"),
      FileRole::Cloth => ("", "yld"),
    };
    format!("{prefix}{slot}{diff}_{index:03}{}.{extension}", self.tail)
  }

  /// The file name as parsed, lowercase.
  pub fn file_name(&self) -> String {
    self.renamed(self.prefix.as_deref(), self.index)
  }
}

/// The slot a body starts with, and the rest after it.
fn parse_slot(body: &str) -> Option<(Slot, &str)> {
  if let Some(rest) = body.strip_prefix("p_") {
    // The anchor runs up to `_diff_` or to the `_` before the index's digits.
    let end = rest.match_indices('_').map(|(position, _)| position).find(|&position| {
      let after = &rest[position + 1..];
      after.starts_with("diff_") || after.bytes().next().is_some_and(|byte| byte.is_ascii_digit())
    })?;
    let anchor = &rest[..end];
    if anchor.is_empty() {
      return None;
    }
    return Some((Slot::Prop(anchor.to_owned()), &rest[end..]));
  }
  COMPONENT_NAMES
    .iter()
    .position(|name| body.strip_prefix(name).is_some_and(|rest| rest.starts_with('_')))
    .map(|index| (Slot::Component(index), &body[COMPONENT_NAMES[index].len()..]))
}

/// Whether `prop_prefix` is the prop prefix of the collection whose component prefix is
/// `component_prefix`: `{ped}_p_{collection}` for `{ped}_{collection}`, at any `_` of it.
pub fn is_prop_prefix_of(
  prop_prefix: &str,
  component_prefix: &str,
) -> bool {
  component_prefix.match_indices('_').any(|(position, _)| {
    let (ped, collection) = component_prefix.split_at(position);
    prop_prefix.strip_prefix(ped).and_then(|rest| rest.strip_prefix("_p")) == Some(collection)
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parsed(name: &str) -> AddonFileName {
    AddonFileName::parse(name).unwrap_or_else(|| panic!("{name} parses"))
  }

  #[test]
  fn names_split_into_prefix_slot_role_index_and_tail() {
    let name = parsed("mp_m_freemode_01_mp_m_smpl_pack^JBIB_012_U.ydd");
    assert_eq!(name.prefix.as_deref(), Some("mp_m_freemode_01_mp_m_smpl_pack"));
    assert_eq!(name.slot, Slot::Component(11));
    assert_eq!((name.role, name.index, name.tail.as_str()), (FileRole::Drawable, 12, "_u"));

    let name = parsed("mp_m_freemode_01_mp_m_smpl_pack^jbib_diff_012_b_uni.ytd");
    assert_eq!((name.role, name.index, name.tail.as_str()), (FileRole::Texture, 12, "_b_uni"));
    assert_eq!(parsed("pre^accs_001_r_1.ydd").tail, "_r_1");
    assert_eq!(parsed("pre^uppr_003_u.yld").role, FileRole::Cloth);

    let name = parsed("mp_f_freemode_01_p_mp_f_smpl_pack^p_lwrist_004.ydd");
    assert_eq!(name.slot, Slot::Prop("lwrist".into()));
    assert_eq!((name.role, name.index, name.tail.as_str()), (FileRole::Drawable, 4, ""));
    let name = parsed("pre^p_ph_l_hand_diff_002_c.ytd");
    assert_eq!(name.slot, Slot::Prop("ph_l_hand".into()));
    assert_eq!((name.role, name.index, name.tail.as_str()), (FileRole::Texture, 2, "_c"));
    assert_eq!(parsed("pre^hair_1000_u.ydd").index, 1000);

    let loose = parsed("jbib_diff_000_a_uni.ytd");
    assert_eq!(loose.prefix, None);
    assert_eq!(loose.slot, Slot::Component(11));
  }

  #[test]
  fn other_files_are_not_addon_clothing() {
    for name in [
      "mp_m_freemode_01_mp_m_smpl_pack.ymt",
      "pre^jbib_000_u.ytd",
      "pre^jbib_diff_000.ytd",
      "pre^jbib_00_u.ydd",
      "pre^jbib_000.ydd",
      "pre^jbib_000_.ydd",
      "pre^shoe_000_u.ydd",
      "pre^p__000.ydd",
      "^jbib_000_u.ydd",
      "a^b^jbib_000_u.ydd",
      "jbib000_u.ydd",
      "prop_bench_01.ydd",
    ] {
      assert_eq!(AddonFileName::parse(name), None, "{name}");
    }
  }

  #[test]
  fn renaming_replaces_prefix_and_index_only() {
    let name = parsed("old_pack^jbib_diff_007_b_whi.ytd");
    assert_eq!(name.renamed(Some("new_pack"), 31), "new_pack^jbib_diff_031_b_whi.ytd");
    assert_eq!(name.renamed(None, 7), "jbib_diff_007_b_whi.ytd");
    assert_eq!(
      parsed("old_p_pack^p_head_002.ydd").renamed(Some("x_p_y"), 9),
      "x_p_y^p_head_009.ydd"
    );
    for file in ["pre^lowr_000_r.ydd", "pre^p_eyes_diff_000_a.ytd", "teef_010_u.yld"] {
      assert_eq!(parsed(file).file_name(), file);
    }
  }

  #[test]
  fn prop_prefixes_insert_p_between_ped_and_collection() {
    let component = "mp_m_freemode_01_mp_m_smpl_pack";
    assert!(is_prop_prefix_of("mp_m_freemode_01_p_mp_m_smpl_pack", component));
    assert!(!is_prop_prefix_of("mp_m_freemode_01_p_mp_m_other", component));
    assert!(!is_prop_prefix_of(component, component));
  }

  /// Every drawable, texture and cloth file of the local `asset/clothing/source` that parses
  /// renders back to its own name. Replacements of vanilla files, such as tattoo textures, do not
  /// parse.
  #[test]
  #[ignore = "requires local asset/clothing/source"]
  fn local_sources_render_back() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/clothing/source");
    let mut stack = vec![root];
    let (mut parsed, mut failed) = (0, Vec::new());
    while let Some(directory) = stack.pop() {
      for entry in crate::platform::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
          stack.push(path);
          continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
        if let Some(file) = AddonFileName::parse(&name) {
          parsed += 1;
          if file.file_name() != name {
            failed.push(name);
          }
        }
      }
    }
    assert!(parsed > 0);
    assert!(failed.is_empty(), "{parsed} parsed, {} render otherwise: {failed:?}", failed.len());
  }
}
