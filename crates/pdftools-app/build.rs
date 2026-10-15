fn main() {
    // Debug info lets the UI tests find elements by accessible label; release
    // builds leave it out to stay small.
    let debug = std::env::var("PROFILE").map_or(true, |p| p != "release");
    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug);
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI failed to compile");

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
