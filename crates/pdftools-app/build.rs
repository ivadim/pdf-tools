fn main() {
    // The icon and file details Windows shows for pdftools.exe.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../../packaging/icon.ico");
        winresource::WindowsResource::new()
            .set_icon("../../packaging/icon.ico")
            .set("ProductName", "PDF Tools")
            .set("FileDescription", "PDF Tools")
            .compile()
            .expect("Windows resources failed to compile");
    }
}
