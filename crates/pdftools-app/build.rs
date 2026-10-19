fn main() {
    // The icon and file details Windows shows for pdftools.exe.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // From the manifest directory, not the working directory, so this works
        // wherever the build tool runs the script.
        let icon = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../packaging/icon.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        winresource::WindowsResource::new()
            .set_icon(icon.to_str().unwrap())
            .set("ProductName", "PDF Tools")
            .set("FileDescription", "PDF Tools")
            .compile()
            .expect("Windows resources failed to compile");
    }
}
