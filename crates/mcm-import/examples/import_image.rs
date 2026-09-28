//! Print the outline for an image and optionally write a debug overlay:
//! `cargo run --release -p mcm-import --example import_image -- <image> [overlay.png]`
use std::time::Instant;

use image::Rgb;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: import_image <image> [overlay.png]");
    let overlay = args.next();
    let bytes = std::fs::read(&path).expect("read image");
    let t = Instant::now();
    let imported = mcm_import::import_image(&bytes, "图片导入").expect("import");
    eprintln!(
        "import took {:?} (reported {} ms)",
        t.elapsed(),
        imported.elapsed_ms
    );
    println!("{}", imported.outline);
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&imported.report).unwrap()
    );

    if let Some(out) = overlay {
        let img = mcm_import::raster::decode(&bytes).unwrap();
        let a = mcm_import::analyse_image(&img).unwrap();
        let mut canvas = img.clone();
        let mut rect = |r: mcm_import::raster::Rect, c: [u8; 3], t: i32| {
            for k in 0..t {
                let r = r
                    .inflate(-k)
                    .clamp_to(canvas.width() as i32, canvas.height() as i32);
                for x in r.x0..r.x1 {
                    for y in [r.y0, r.y1 - 1] {
                        canvas.put_pixel(x as u32, y as u32, Rgb(c));
                    }
                }
                for y in r.y0..r.y1 {
                    for x in [r.x0, r.x1 - 1] {
                        canvas.put_pixel(x as u32, y as u32, Rgb(c));
                    }
                }
            }
        };
        for t in &a.text {
            rect(t.rect, [0, 120, 255], 1);
        }
        for b in &a.boxes {
            rect(b.rect, [255, 0, 0], 3);
        }
        for f in &a.frames {
            rect(f.rect, [255, 0, 255], 3);
        }
        for e in &a.diagram.edges {
            let (ax, ay) = a.diagram.nodes[e.from].rect.center();
            let (bx, by) = a.diagram.nodes[e.to].rect.center();
            let n = (ax - bx).abs().max((ay - by).abs()).max(1);
            for i in 0..=n {
                let x = ax + (bx - ax) * i / n;
                let y = ay + (by - ay) * i / n;
                // Fade from dark (tail) to bright red (head).
                let c = (80 + 175 * i / n) as u8;
                for d in -1..=1 {
                    canvas.put_pixel((x + d) as u32, y as u32, Rgb([c, 0, 0]));
                }
            }
        }
        canvas.save(&out).unwrap();
    }
}
