//! Catalog of OCR model files that are *not* compiled in.
//!
//! The fast recognizer ships inside the binary. The accurate one is 21 MB —
//! bundling it would break the 25 MB installer budget (宪法 II) — so it is
//! downloaded on explicit request (本地优先：网络能力默认关闭) and pinned by
//! SHA-256 here. This crate only verifies; fetching lives in the app shell.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::ImportError;

/// One file of a downloadable model, pinned by size and digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ModelFile {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// PP-OCRv6 small recognizer and its dictionary (Apache-2.0, PaddleOCR).
/// Same digests as the ModelScope `greatv/oar-ocr` registry and the
/// oar-ocr v0.7.0 GitHub release.
pub const ACCURATE_FILES: [ModelFile; 2] = [
    ModelFile {
        name: "pp-ocrv6_small_rec.onnx",
        size: 21_159_378,
        sha256: "5435fd747c9e0efe15a96d0b378d5bd157e9492ed8fd80edf08f30d02fa24634",
    },
    ModelFile {
        name: "ppocrv6_dict.txt",
        size: 74_947,
        sha256: "b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d",
    },
];

/// Download sources tried in order; `{name}` is replaced by the file name.
/// ModelScope first (fast in mainland China), GitHub as the fallback.
pub const ACCURATE_MIRRORS: [&str; 2] = [
    "https://www.modelscope.cn/api/v1/models/greatv/oar-ocr/repo?Revision=master&FilePath={name}",
    "https://github.com/GreatV/oar-ocr/releases/download/v0.7.0/{name}",
];

/// Total bytes to download for the accurate model.
#[must_use]
pub fn accurate_download_size() -> u64 {
    ACCURATE_FILES.iter().map(|f| f.size).sum()
}

/// Lower-case hex SHA-256.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Check bytes against the pinned size and digest.
///
/// # Errors
/// [`ImportError::Model`] naming the file and what did not match.
pub fn verify(file: &ModelFile, bytes: &[u8]) -> Result<(), ImportError> {
    if bytes.len() as u64 != file.size {
        return Err(ImportError::Model(format!(
            "{} 大小不符：应为 {} 字节，实际 {} 字节",
            file.name,
            file.size,
            bytes.len()
        )));
    }
    let actual = sha256_hex(bytes);
    if actual != file.sha256 {
        return Err(ImportError::Model(format!(
            "{} 校验和不符（{actual}）",
            file.name
        )));
    }
    Ok(())
}

/// Every file present with the expected size. Cheap enough for a status
/// check; the digest is verified when the model is actually loaded.
#[must_use]
pub fn accurate_installed(dir: &Path) -> bool {
    ACCURATE_FILES.iter().all(|f| {
        std::fs::metadata(dir.join(f.name)).is_ok_and(|m| m.is_file() && m.len() == f.size)
    })
}

/// Read and verify one file of the accurate model from `dir`.
///
/// # Errors
/// [`ImportError::Model`] when the file is missing or does not match.
pub fn read_verified(dir: &Path, file: &ModelFile) -> Result<Vec<u8>, ImportError> {
    let path = dir.join(file.name);
    let bytes = std::fs::read(&path)
        .map_err(|e| ImportError::Model(format!("缺少高精度模型文件 {}：{e}", path.display())))?;
    verify(file, &bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: ModelFile = ModelFile {
        name: "x.txt",
        size: 3,
        // sha256("abc")
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    };

    #[test]
    fn verify_accepts_exact_bytes_and_rejects_tampering() {
        assert!(verify(&TINY, b"abc").is_ok());
        let wrong_hash = verify(&TINY, b"abd").unwrap_err().to_string();
        assert!(wrong_hash.contains("校验和"), "{wrong_hash}");
        let wrong_size = verify(&TINY, b"abcd").unwrap_err().to_string();
        assert!(wrong_size.contains("大小"), "{wrong_size}");
    }

    #[test]
    fn a_missing_directory_is_not_installed() {
        assert!(!accurate_installed(Path::new("/definitely/not/here")));
        assert!(accurate_download_size() > 21_000_000);
    }

    #[test]
    fn mirrors_template_the_file_name() {
        for mirror in ACCURATE_MIRRORS {
            assert!(mirror.starts_with("https://") && mirror.contains("{name}"));
        }
    }
}
