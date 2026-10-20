//! Metadata read/edit via ExifTool.

use crate::im::abs;
use crate::params::str_or;
use crate::util::{finish_named, s};
use orange_core::kind::ext_of;
use orange_core::paths::stem_of;
use orange_core::{exec, Capabilities, Ctx, Dep, Error, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn read(path: &Path, caps: &Capabilities) -> Result<Value> {
    let ctx = Ctx::new(Arc::new(caps.clone()));
    let et = ctx.caps.require(Dep::Exiftool)?;
    let mut c = exec::command(et);
    c.args(["-j", "-n", "-api", "largefilesupport=1"]).arg(abs(path));
    let out = exec::run(&mut c, "ExifTool", &ctx)?;
    let v: Value = serde_json::from_str(&out.stdout)?;
    let o = v.get(0).cloned().unwrap_or_else(|| json!({}));
    let pick = |keys: &[&str]| keys.iter().find_map(|k| o.get(*k).filter(|v| !v.is_null()).cloned()).unwrap_or(Value::Null);
    let gps = match (o.get("GPSLatitude").and_then(|v| v.as_f64()), o.get("GPSLongitude").and_then(|v| v.as_f64())) {
        (Some(lat), Some(lon)) => json!({"lat":lat,"lon":lon}),
        _ => Value::Null,
    };
    let mut raw = o.clone();
    if let Some(m) = raw.as_object_mut() {
        m.remove("SourceFile");
    }
    Ok(json!({
        "make": pick(&["Make"]),
        "model": pick(&["Model"]),
        "lens": pick(&["LensModel", "LensID", "Lens"]),
        "taken": pick(&["DateTimeOriginal", "CreateDate"]),
        "gps": gps,
        "artist": pick(&["Artist", "Creator"]),
        "copyright": pick(&["Copyright", "Rights"]),
        "description": pick(&["ImageDescription", "Description"]),
        "software": pick(&["Software"]),
        "raw": raw,
    }))
}

const SETTABLE: &[&str] = &["Make", "Model", "LensModel", "DateTimeOriginal", "CreateDate", "Artist", "Copyright", "ImageDescription", "Software"];

fn build_args(params: &Value) -> Result<Vec<String>> {
    let mut a: Vec<String> = Vec::new();
    if let Some(set) = params.get("set").and_then(|v| v.as_object()) {
        for (k, v) in set {
            if k == "GPSLatitude" || k == "GPSLongitude" {
                continue;
            }
            if !SETTABLE.contains(&k.as_str()) {
                return Err(Error::BadParams(format!("tag not editable: {k}")));
            }
            let val = match v {
                Value::Null => String::new(),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            a.push(format!("-{k}={val}"));
        }
        let lat = set.get("GPSLatitude").and_then(|v| v.as_f64());
        let lon = set.get("GPSLongitude").and_then(|v| v.as_f64());
        match (lat, lon) {
            (Some(la), Some(lo)) => {
                if !(-90.0..=90.0).contains(&la) || !(-180.0..=180.0).contains(&lo) {
                    return Err(Error::BadParams("GPS coordinates out of range".into()));
                }
                a.push(format!("-GPSLatitude={}", la.abs()));
                a.push(format!("-GPSLatitudeRef={}", if la < 0.0 { "S" } else { "N" }));
                a.push(format!("-GPSLongitude={}", lo.abs()));
                a.push(format!("-GPSLongitudeRef={}", if lo < 0.0 { "W" } else { "E" }));
            }
            (None, None) => {}
            _ => return Err(Error::BadParams("set both GPSLatitude and GPSLongitude".into())),
        }
    }
    if let Some(strip) = params.get("strip").and_then(|v| v.as_array()) {
        for item in strip {
            match item.as_str().unwrap_or("") {
                "gps" => a.push("-gps:all=".into()),
                "camera" => {
                    for t in ["Make", "Model", "LensModel", "LensMake", "SerialNumber", "BodySerialNumber", "LensSerialNumber", "Software"] {
                        a.push(format!("-{t}="));
                    }
                }
                "all" => a.push("-all=".into()),
                other => return Err(Error::BadParams(format!("unknown strip target: {other}"))),
            }
        }
    }
    if a.is_empty() {
        return Err(Error::BadParams("nothing to change".into()));
    }
    Ok(a)
}

pub fn edit(inputs: &[PathBuf], params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let et = ctx.caps.require(Dep::Exiftool)?;
    let args = build_args(params)?;
    let in_place = crate::params::bool_or(params, "in_place", false);
    let _ = str_or(params, "x", "");
    let mut outs = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        ctx.check_cancel()?;
        ctx.report(i as f32 / inputs.len() as f32, format!("Updating metadata ({}/{})", i + 1, inputs.len()));
        let (target, td) = if in_place {
            (input.clone(), None)
        } else {
            let td = ctx.temp_dir()?;
            let t = td.path().join(format!("work.{}", ext_of(input)));
            std::fs::copy(input, &t)?;
            (t, Some(td))
        };
        let mut c = exec::command(et);
        c.arg("-overwrite_original").args(&args).arg(abs(&target));
        exec::run(&mut c, "ExifTool", ctx)?;
        if in_place {
            outs.push(target);
        } else {
            let ext = ext_of(input);
            outs.push(finish_named(&target, input, &format!("{} Edited", stem_of(input)), &ext, ctx)?);
        }
        drop(td);
    }
    ctx.report(1.0, "Done");
    let _ = s;
    Ok(outs)
}
