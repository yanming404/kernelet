fn main() {
    let root = std::env::var("ASTERINAS_DIR").unwrap_or_else(|_| "../../../../asterinas".into());
    let file = std::fs::canonicalize(
        std::path::Path::new(&root).join("kernel/core/src/net/socket/vsock/typedio.rs"),
    )
    .unwrap();
    println!("cargo:rerun-if-changed={}", file.display());
    let text = format!("#[path={:?}] pub mod typedio;", file);
    std::fs::write(
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("adapter.rs"),
        text,
    )
    .unwrap();
}
