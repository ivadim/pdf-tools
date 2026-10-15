fn main() {
    // Debug info lets the UI tests find elements by accessible label; release
    // builds leave it out to stay small.
    let debug = std::env::var("PROFILE").map_or(true, |p| p != "release");
    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug);
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI failed to compile");
}
