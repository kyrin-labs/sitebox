//! Optional screenshots.
//!
//! A browser is not a dependency of the engine. When one is present the agent
//! can look; when one is not, `sitebox_shot` says so and the turn continues with
//! checks only. "Verified visually" is never claimed without a file on disk.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

use super::{ensure_dir, new_id, StudioState};

pub fn browser() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CHROME") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    for name in [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "chrome",
        "/usr/bin/chromium",
        "/usr/bin/google-chrome",
    ] {
        if let Ok(out) = std::process::Command::new("which").arg(name).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
    }
    None
}

pub async fn capture(
    st: &StudioState,
    site_id: &str,
    port: u16,
    width: u32,
    height: u32,
    route: &str,
) -> Result<Value> {
    let Some(chrome) = browser() else {
        bail!("no Chromium/Chrome found on this host (set CHROME to a browser binary to enable screenshots)");
    };
    let width = width.clamp(320, 2560);
    let height = height.clamp(320, 2000);
    let route = if route.starts_with('/') {
        route.to_string()
    } else {
        format!("/{route}")
    };
    let dir = st.data.join("shots").join(site_id);
    ensure_dir(&dir)?;
    let file = dir.join(format!("{}-{width}.png", new_id("shot")));
    let url = format!("http://127.0.0.1:{port}{route}");

    let mut last_err = String::new();
    for headless in ["--headless=new", "--headless"] {
        let out = tokio::time::timeout(
            Duration::from_secs(40),
            tokio::process::Command::new(&chrome)
                .args([
                    headless,
                    "--disable-gpu",
                    "--no-sandbox",
                    "--hide-scrollbars",
                    "--disable-dev-shm-usage",
                    "--force-device-scale-factor=1",
                    &format!("--window-size={width},{height}"),
                    &format!("--screenshot={}", file.display()),
                    &url,
                ])
                .output(),
        )
        .await;
        match out {
            Ok(Ok(o)) if o.status.success() && file.is_file() => {
                return Ok(json!({
                    "ok": true,
                    "path": file,
                    "url": format!("/api/studio/shots/{site_id}/{}", file.file_name().unwrap_or_default().to_string_lossy()),
                    "width": width,
                    "height": height,
                    "chrome": chrome,
                }));
            }
            Ok(Ok(o)) => {
                last_err = format!(
                    "{headless} exit {}: {}",
                    o.status,
                    String::from_utf8_lossy(&o.stderr)
                        .chars()
                        .take(300)
                        .collect::<String>()
                );
            }
            Ok(Err(e)) => last_err = format!("spawn {}: {e}", chrome.display()),
            Err(_) => last_err = format!("{headless} timed out after 40s"),
        }
    }
    Err(anyhow::anyhow!("screenshot failed: {last_err}")).context("chromium did not produce a file")
}

/// Render a URL in the same browser used for screenshots and return the DOM
/// after scripts have run. Static fetching sees an empty shell on JS apps; this
/// is what lets the agent read a modern site instead of guessing from snippets.
pub async fn dump_dom(url: &str) -> Result<String> {
    dump_dom_at(url, 1280, 900).await
}

/// Same, with the viewport the measurement is about. Mobile overflow is a
/// different page than desktop, so the width is not optional for a probe.
pub async fn dump_dom_at(url: &str, width: u32, height: u32) -> Result<String> {
    let Some(chrome) = browser() else {
        bail!("no Chromium/Chrome found on this host (set CHROME to enable rendering)");
    };
    // Headless Chrome refuses to go narrower than 500 CSS pixels; asking for
    // 375 silently gives 500. The probe reports the viewport it actually got so
    // nobody draws conclusions from a width the browser never rendered.
    let size = format!("--window-size={},{height}", width.max(500));
    let mut last_err = String::new();
    for headless in ["--headless=new", "--headless"] {
        let out = tokio::time::timeout(
            Duration::from_secs(30),
            tokio::process::Command::new(&chrome)
                .args([
                    headless,
                    "--disable-gpu",
                    "--no-sandbox",
                    "--disable-dev-shm-usage",
                    "--virtual-time-budget=6000",
                    &size,
                    "--dump-dom",
                    url,
                ])
                .output(),
        )
        .await;
        match out {
            Ok(Ok(o)) if o.status.success() => {
                let dom = String::from_utf8_lossy(&o.stdout).to_string();
                if dom.len() > 200 {
                    return Ok(dom);
                }
                last_err = format!("{headless} produced {} bytes", dom.len());
            }
            Ok(Ok(o)) => {
                last_err = format!(
                    "{headless} exit {}: {}",
                    o.status,
                    String::from_utf8_lossy(&o.stderr)
                        .chars()
                        .take(200)
                        .collect::<String>()
                )
            }
            Ok(Err(e)) => last_err = format!("spawn {}: {e}", chrome.display()),
            Err(_) => last_err = format!("{headless} timed out after 30s"),
        }
    }
    bail!("render failed for {url}: {last_err}")
}
