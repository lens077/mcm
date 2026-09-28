//! The downloadable PP-OCRv6 small recognizer. Not part of the default run:
//! the model is 21 MB and is not checked in.
//!
//! ```text
//! MCM_ACCURATE_MODEL_DIR=<dir with the two files> cargo test -p mcm-import -- --ignored
//! ```

use std::path::PathBuf;

use mcm_import::{OcrEngine, import_image, import_image_with};

const PNG: &[u8] = include_bytes!("../fixtures/archify-go-service.png");

fn model_dir() -> PathBuf {
    PathBuf::from(
        std::env::var_os("MCM_ACCURATE_MODEL_DIR")
            .expect("set MCM_ACCURATE_MODEL_DIR to the downloaded model directory"),
    )
}

#[test]
#[ignore = "需要下载高精度模型，见文件头注释"]
fn accurate_model_fixes_the_fast_models_slips() {
    let engine = OcrEngine::accurate(&model_dir()).expect("load accurate model");
    let accurate = import_image_with(PNG, "x", &engine).unwrap().outline;
    // Exactly the three slips the tiny recognizer makes on this screenshot.
    for exact in [
        "- PostgreSQL #",
        "> sqlc 生成的 Querier",
        "> discovery:///<注册名>",
    ] {
        assert!(accurate.contains(exact), "missing {exact:?} in\n{accurate}");
    }
    // Structure does not depend on the recognizer.
    let fast = import_image(PNG, "x").unwrap();
    let accurate = import_image_with(PNG, "x", &engine).unwrap();
    assert_eq!(fast.report.nodes, accurate.report.nodes);
    assert_eq!(fast.report.groups, accurate.report.groups);
    assert_eq!(fast.report.dependencies, accurate.report.dependencies);
}

#[test]
fn a_directory_without_the_model_is_refused_clearly() {
    let err = OcrEngine::accurate(std::path::Path::new("/definitely/not/here"))
        .err()
        .unwrap();
    assert!(err.to_string().contains("缺少高精度模型文件"), "{err}");
}
