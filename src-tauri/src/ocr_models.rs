//! On-demand download of the accurate OCR model.
//!
//! Network access happens only here, only when the user clicks download
//! (宪法：网络能力是显式可选项，默认关闭). Each file is fetched whole,
//! verified against the digest pinned in `mcm_import::models`, written to a
//! `.part` file and renamed into place — a failed or tampered download never
//! leaves a file that looks installed.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mcm_import::models::{self, ModelFile};

const CHUNK: usize = 64 * 1024;

/// Where the accurate model lives inside the app data directory.
#[must_use]
pub fn accurate_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("pp-ocrv6-small")
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        // ureq limits the whole body, not a single read. Keep it generous so a
        // slow mirror (21 MB at ~20 KB/s) still finishes; it only has to stop
        // a connection that has truly hung.
        .timeout_recv_body(Some(Duration::from_secs(20 * 60)))
        .build()
        .new_agent()
}

/// Download every missing file of `files` into `dir`, trying `mirrors` in
/// order (`{name}` in a mirror is replaced by the file name). `progress`
/// receives (bytes done, bytes total) across all files.
///
/// # Errors
/// A readable message listing why each mirror failed for the first file
/// that could not be obtained.
pub fn download(
    dir: &Path,
    files: &[ModelFile],
    mirrors: &[&str],
    mut progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("无法创建模型目录 {}：{e}", dir.display()))?;
    let total: u64 = files.iter().map(|f| f.size).sum();
    let mut done = 0u64;
    let agent = agent();
    for file in files {
        let target = dir.join(file.name);
        // Already there and intact (e.g. an interrupted earlier run): skip.
        if std::fs::read(&target).is_ok_and(|bytes| models::verify(file, &bytes).is_ok()) {
            done += file.size;
            progress(done, total);
            continue;
        }
        let mut failures = Vec::new();
        let mut fetched = None;
        for mirror in mirrors {
            let url = mirror.replace("{name}", file.name);
            match fetch(&agent, &url, file, |n| progress(done + n, total)) {
                Ok(bytes) => {
                    fetched = Some(bytes);
                    break;
                }
                Err(reason) => failures.push(format!("{}：{reason}", host_of(&url))),
            }
        }
        let Some(bytes) = fetched else {
            return Err(format!(
                "下载 {} 失败（{}）",
                file.name,
                failures.join("；")
            ));
        };
        let part = dir.join(format!("{}.part", file.name));
        std::fs::write(&part, &bytes)
            .and_then(|()| std::fs::rename(&part, &target))
            .map_err(|e| format!("无法写入 {}：{e}", target.display()))?;
        done += file.size;
        progress(done, total);
    }
    Ok(())
}

fn fetch(
    agent: &ureq::Agent,
    url: &str,
    file: &ModelFile,
    mut progress: impl FnMut(u64),
) -> Result<Vec<u8>, String> {
    let response = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut reader = response.into_body().into_reader();
    let capacity = usize::try_from(file.size).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        // A server sending more than the pinned size is wrong; stop early.
        if bytes.len() as u64 > file.size {
            return Err("返回内容超过预期大小".into());
        }
        progress(bytes.len() as u64);
    }
    models::verify(file, &bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn host_of(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// Serves `body` for every request, `count` times, then stops.
    fn serve(body: &'static [u8], count: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(count) {
                let mut stream = stream.unwrap();
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body);
            }
        });
        format!("http://{addr}/{{name}}")
    }

    const ABC: ModelFile = ModelFile {
        name: "abc.txt",
        size: 3,
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    };

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn scratch(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("mcm-ocr-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Scratch(dir)
    }

    #[test]
    fn tampered_mirror_falls_back_to_the_next_one() {
        let bad = serve(b"abd", 1);
        let good = serve(b"abc", 1);
        let dir = scratch("fallback");
        let mut last = (0, 0);
        download(&dir.0, &[ABC], &[&bad, &good], |d, t| last = (d, t)).unwrap();
        assert_eq!(std::fs::read(dir.0.join("abc.txt")).unwrap(), b"abc");
        assert!(!dir.0.join("abc.txt.part").exists());
        assert_eq!(last, (3, 3));
    }

    #[test]
    fn nothing_is_installed_when_every_mirror_is_wrong() {
        let bad = serve(b"xyz", 1);
        let dir = scratch("allbad");
        let error = download(&dir.0, &[ABC], &[&bad], |_, _| {}).unwrap_err();
        assert!(error.contains("校验和"), "{error}");
        assert!(!dir.0.join("abc.txt").exists());
    }

    /// Real network: `cargo test -p mcm-app real_mirrors -- --ignored`.
    #[test]
    #[ignore = "访问公网下载 21 MB"]
    fn real_mirrors_serve_the_pinned_files() {
        use mcm_import::models::{ACCURATE_FILES, ACCURATE_MIRRORS};
        // Primary mirror: the whole model, loaded as an engine.
        let dir = scratch("real-primary");
        download(&dir.0, &ACCURATE_FILES, &ACCURATE_MIRRORS[..1], |_, _| {}).unwrap();
        mcm_import::OcrEngine::accurate(&dir.0).map(|_| ()).unwrap();
        // Fallbacks: the small dictionary proves URL layout and digest.
        for mirror in &ACCURATE_MIRRORS[1..] {
            let dir = scratch(&format!("real-{}", host_of(mirror)));
            download(&dir.0, &ACCURATE_FILES[1..], &[mirror], |_, _| {}).unwrap();
        }
    }

    #[test]
    fn intact_files_are_not_downloaded_again() {
        let dir = scratch("skip");
        std::fs::create_dir_all(&dir.0).unwrap();
        std::fs::write(dir.0.join("abc.txt"), b"abc").unwrap();
        // No server at all: success proves nothing was fetched.
        download(&dir.0, &[ABC], &["http://127.0.0.1:9/{name}"], |_, _| {}).unwrap();
    }
}
