//! `cargo run --release -p mcm-import --example ocr_dump -- <image>`
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("usage: ocr_dump <image>");
    let bytes = std::fs::read(&path).expect("read image");
    let img = mcm_import::raster::decode(&bytes).expect("decode");
    let t = Instant::now();
    let engine = mcm_import::ocr::OcrEngine::shared().expect("engine");
    let load = t.elapsed();
    for _ in 0..2 {
        let t = Instant::now();
        let lines = engine.read(&img).expect("ocr");
        eprintln!("load {load:?} read {:?} lines {}", t.elapsed(), lines.len());
        if std::env::var("QUIET").is_err() {
            for l in &lines {
                println!(
                    "{:.2}\t{},{}\t{}",
                    l.confidence, l.rect.x0, l.rect.y0, l.text
                );
            }
        }
    }
}
