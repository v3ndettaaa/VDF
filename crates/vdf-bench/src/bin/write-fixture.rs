//! One-off fixture writer: `cargo run -p vdf-bench --bin write-fixture -- <path> <pages>`
#[path = "../../../vdf-pdf/tests/support/mod.rs"]
mod fixture;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: write-fixture <path> <pages>");
    let pages: usize = args.get(2).map(|p| p.parse().expect("pages")).unwrap_or(30);
    std::fs::write(path, fixture::make_pdf(pages)).expect("write");
    println!("wrote {path} ({pages} pages)");
}
