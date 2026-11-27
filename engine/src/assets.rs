//! Image pipeline.
//!
//! The rule that matters, learned from Folio: **never bake a crop into a
//! media asset**. This module can only downscale; the one exception is an
//! explicitly square avatar, because a circular avatar *is* the form.
//!
//! Output is lossy WebP via libwebp, and `dims.json` is written from the
//! encoded files — never recomputed from assumptions, so "declared 405x720 for
//! a 1080x1920 frame" cannot happen.

use anyhow::{bail, Context, Result};
use image::imageops::FilterType;
use image::GenericImageView;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct AssetRules {
    pub max_edge: u32,
    pub quality: f32,
    pub square_avatars: bool,
    pub force: bool,
}

impl Default for AssetRules {
    fn default() -> Self {
        Self {
            max_edge: 1080,
            quality: 82.0,
            square_avatars: true,
            force: false,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct AssetReport {
    pub inputs: usize,
    pub written: usize,
    pub skipped: usize,
    pub failed: usize,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub dims_written: usize,
    pub errors: Vec<String>,
}

const EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif"];

pub fn build_assets(
    input_dir: &Path,
    public_dir: &Path,
    out_rel: &str,
    dims_path: &Path,
    rules: &AssetRules,
) -> Result<AssetReport> {
    if !input_dir.is_dir() {
        bail!("input folder missing: {}", input_dir.display());
    }
    let out_dir = public_dir.join(out_rel);
    std::fs::create_dir_all(&out_dir)?;

    let mut files: Vec<PathBuf> = Vec::new();
    for e in walkdir::WalkDir::new(input_dir).max_depth(4) {
        let e = e?;
        if !e.file_type().is_file() {
            continue;
        }
        let ext = e
            .path()
            .extension()
            .map(|x| x.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if EXTS.contains(&ext.as_str()) {
            files.push(e.path().to_path_buf());
        }
    }
    files.sort();

    let mut report = AssetReport {
        inputs: files.len(),
        ..Default::default()
    };
    let errors = Mutex::new(Vec::new());
    let results: Vec<(String, u32, u32, u64, u64, bool)> = files
        .par_iter()
        .filter_map(|src| match convert_one(src, &out_dir, rules) {
            Ok(Some((name, w, h, bin, bout, skipped))) => Some((name, w, h, bin, bout, skipped)),
            Ok(None) => None,
            Err(e) => {
                errors
                    .lock()
                    .unwrap()
                    .push(format!("{}: {e}", src.display()));
                None
            }
        })
        .collect();

    // dims.json is keyed by path relative to `public/`, e.g. `img/av-x.webp`.
    let mut dims: BTreeMap<String, serde_json::Value> = if dims_path.is_file() {
        serde_json::from_str(&std::fs::read_to_string(dims_path)?).unwrap_or_default()
    } else {
        BTreeMap::new()
    };

    for (name, w, h, bin, bout, skipped) in results {
        report.bytes_in += bin;
        report.bytes_out += bout;
        if skipped {
            report.skipped += 1;
        } else {
            report.written += 1;
        }
        let key = format!("{}/{}", out_rel.trim_end_matches('/'), name);
        // Keep an existing colour (index 3) if the old file had one.
        let colour = dims
            .get(&key)
            .and_then(|v| v.as_array())
            .and_then(|a| a.get(3))
            .cloned();
        let ratio = (w as f64) / (h.max(1) as f64);
        let mut arr = vec![
            serde_json::json!(w),
            serde_json::json!(h),
            serde_json::json!((ratio * 1_000_000.0).round() / 1_000_000.0),
        ];
        if let Some(c) = colour {
            arr.push(c);
        }
        dims.insert(key, serde_json::Value::Array(arr));
        report.dims_written += 1;
    }

    if let Some(parent) = dims_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dims_path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&dims)?)?;
    std::fs::rename(&tmp, dims_path)?;

    report.errors = errors.into_inner().unwrap_or_default();
    report.failed = report.errors.len();
    Ok(report)
}

type Converted = (String, u32, u32, u64, u64, bool);

fn convert_one(src: &Path, out_dir: &Path, rules: &AssetRules) -> Result<Option<Converted>> {
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
    let out = out_dir.join(format!("{stem}.webp"));

    let in_meta = std::fs::metadata(src)?;
    let in_bytes = in_meta.len();
    if !rules.force && out.is_file() {
        let out_meta = std::fs::metadata(&out)?;
        if out_meta.modified().ok() >= in_meta.modified().ok() {
            let (w, h) = image::image_dimensions(&out)?;
            return Ok(Some((
                format!("{stem}.webp"),
                w,
                h,
                in_bytes,
                out_meta.len(),
                true,
            )));
        }
    }

    let img = image::open(src).with_context(|| format!("decode {}", src.display()))?;
    let (w0, h0) = img.dimensions();
    if w0 == 0 || h0 == 0 {
        bail!("zero-sized image");
    }

    let is_avatar = rules.square_avatars && (stem.starts_with("av-") || stem.contains("avatar"));

    let img = if is_avatar {
        // Avatars are square by form: centre-crop, then scale down.
        let side = w0.min(h0);
        let x = (w0 - side) / 2;
        let y = (h0 - side) / 2;
        let square = img.crop_imm(x, y, side, side);
        let target = rules.max_edge.min(160).min(side);
        square.resize(target, target, FilterType::Lanczos3)
    } else {
        let longest = w0.max(h0);
        if longest <= rules.max_edge {
            img
        } else {
            let scale = rules.max_edge as f64 / longest as f64;
            let nw = ((w0 as f64) * scale).round().max(1.0) as u32;
            let nh = ((h0 as f64) * scale).round().max(1.0) as u32;
            img.resize(nw, nh, FilterType::Lanczos3)
        }
    };

    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8();
    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), w, h);
    let encoded = encoder.encode(rules.quality);
    std::fs::write(&out, &*encoded)?;
    let out_bytes = encoded.len() as u64;
    Ok(Some((
        format!("{stem}.webp"),
        w,
        h,
        in_bytes,
        out_bytes,
        false,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_preserves_aspect() {
        let dir = std::env::temp_dir().join(format!("sbx-assets-{}", std::process::id()));
        let input = dir.join("raw");
        let public = dir.join("public");
        std::fs::create_dir_all(&input).unwrap();
        std::fs::create_dir_all(&public).unwrap();
        let img = image::RgbImage::from_fn(200, 100, |x, _| image::Rgb([x as u8, 40, 60]));
        img.save(input.join("banner.png")).unwrap();
        let dims = dir.join("dims.json");
        let rep = build_assets(
            &input,
            &public,
            "img",
            &dims,
            &AssetRules {
                max_edge: 100,
                quality: 80.0,
                square_avatars: true,
                force: true,
            },
        )
        .unwrap();
        assert_eq!(rep.written, 1, "{rep:?}");
        let (w, h) = image::image_dimensions(public.join("img/banner.webp")).unwrap();
        assert_eq!((w, h), (100, 50));
        let d: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&dims).unwrap()).unwrap();
        assert_eq!(d["img/banner.webp"][0], 100);
        assert_eq!(d["img/banner.webp"][1], 50);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
