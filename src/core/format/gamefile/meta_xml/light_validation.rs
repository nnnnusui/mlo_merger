use super::{invalid_data, io};
use crate::core::format::ymap::xml::XmlYmap;

pub(super) fn validate(parsed: &XmlYmap) -> io::Result<()> {
  let lod = &parsed.lod_lights_soa;
  if lod.error.as_ref().is_some_and(|error| !error.trim().is_empty()) {
    return Err(invalid_data("LODLightsSOA contains a decoding error"));
  }
  let count = lod.direction.items.len();
  for (name, text) in [
    ("falloff", &lod.falloff.text),
    ("falloffExponent", &lod.falloff_exponent.text),
    ("timeAndStateFlags", &lod.time_and_state_flags.text),
    ("hash", &lod.hash.text),
    ("coneInnerAngle", &lod.cone_inner_angle.text),
    ("coneOuterAngleOrCapExt", &lod.cone_outer_angle_or_cap_ext.text),
    ("coronaIntensity", &lod.corona_intensity.text),
  ] {
    if text.split_whitespace().count() != count {
      return Err(invalid_data(&format!("LODLightsSOA.{name} length differs from direction")));
    }
    for token in text.split_whitespace() {
      if matches!(name, "hash" | "timeAndStateFlags" | "coneInnerAngle" | "coneOuterAngleOrCapExt")
      {
        token.parse::<u32>().map_err(|_| invalid_data("Invalid integer light property"))?;
      } else {
        let value =
          token.parse::<f32>().map_err(|_| invalid_data("Invalid scalar light property"))?;
        if !value.is_finite() {
          return Err(invalid_data("Nonfinite light property"));
        }
      }
    }
  }
  let distant = &parsed.distant_lod_lights_soa;
  if distant.error.as_ref().is_some_and(|error| !error.trim().is_empty()) {
    return Err(invalid_data("DistantLODLightsSOA contains a decoding error"));
  }
  if distant.rgbi.text.split_whitespace().count() != distant.position.items.len() {
    return Err(invalid_data("DistantLODLightsSOA position/RGBI lengths differ"));
  }
  for token in distant.rgbi.text.split_whitespace() {
    token.parse::<u32>().map_err(|_| invalid_data("Invalid distant light RGBI"))?;
  }
  Ok(())
}
