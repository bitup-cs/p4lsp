use std::path::PathBuf;

fn main() {
    let dir: PathBuf = ["..", "tree-sitter-p4", "src"].iter().collect();

    cc::Build::new()
        .include(&dir)
        .file(dir.join("parser.c"))
        .compile("tree-sitter-p4");

    println!("cargo:rerun-if-changed={}", dir.display());
}
