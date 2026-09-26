fn main() {
    // `include_dir!` embeds the store's migrations, but on stable Rust
    // the compiler only notices edits to files it already embedded, not a
    // migration added beside them. Rebuild when the directory changes.
    println!("cargo:rerun-if-changed=src/store/migrations");
}
