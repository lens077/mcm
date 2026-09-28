//! `cargo run --release -p mcm-import --example import_html -- <file.html>`
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: import_html <file.html>");
    let bytes = std::fs::read(&path).expect("read html");
    let imported = mcm_import::import_html(&bytes, "HTML 导入").expect("import");
    println!("{}", imported.outline);
    eprintln!("{} ms", imported.elapsed_ms);
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&imported.report).unwrap()
    );
}
