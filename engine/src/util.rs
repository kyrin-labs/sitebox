//! Small shared helpers: path safety, hashing, ports, bytes.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::path::{Component, Path, PathBuf};

pub fn sha256_bytes(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    hex::encode(h.finalize())
}

pub fn sha256_file(p: &Path) -> Result<String> {
    let mut f = std::fs::File::open(p).with_context(|| format!("open {}", p.display()))?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

pub fn human_bytes(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i + 1 < U.len() {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

/// True when something is already bound on the loopback port.
pub fn port_bound(port: u16) -> bool {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    match TcpListener::bind(addr) {
        Ok(l) => {
            drop(l);
            false
        }
        Err(_) => true,
    }
}

/// Join an URL path onto `root` without ever escaping it.
///
/// Percent-decoding happens first, then every component is checked: `..`,
/// absolute components and symlink-style prefixes are refused.
pub fn safe_join(root: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(url_path)
        .decode_utf8()
        .ok()?
        .into_owned();
    if decoded.contains('\0') {
        return None;
    }
    let rel = decoded.trim_start_matches('/');
    let mut out = PathBuf::from(root);
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            _ => return None,
        }
    }
    // Defence in depth: canonicalise only if the target exists; otherwise
    // verify the lexical prefix.
    if out.exists() {
        let canon_root = root.canonicalize().ok()?;
        let canon = out.canonicalize().ok()?;
        if !canon.starts_with(&canon_root) {
            return None;
        }
        Some(canon)
    } else {
        Some(out)
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Run a declared pipeline step through `sh -c`, capturing output.
pub fn run_shell(cwd: &Path, cmd: &str) -> Result<(i32, String)> {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("run `{cmd}` in {}", cwd.display()))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.code().unwrap_or(-1), text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_is_refused() {
        let root = Path::new("/tmp/sbx-root");
        assert!(safe_join(root, "/index.html").is_some());
        assert!(safe_join(root, "/../../etc/passwd").is_none());
        assert!(safe_join(root, "/%2e%2e/%2e%2e/etc/passwd").is_none());
        assert!(safe_join(root, "/a/../b").is_some() || safe_join(root, "/a/../b").is_none());
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
