# R8 rules for release builds. AndroidX brings its own.

# The engine (crates/pdftools-ffi/src/android.rs) calls these through JNI:
# the native methods, and the constructors of what it returns or throws.
-keepclasseswithmembernames class io.github.ivadim.pdftools.Engine {
    native <methods>;
}
-keep class io.github.ivadim.pdftools.NativePage {
    <init>(int, int, float, float, byte[]);
}
-keep class io.github.ivadim.pdftools.EngineException {
    <init>(java.lang.String);
}

# Provided by the device's system image (foldables), not the app.
-dontwarn androidx.window.extensions.**
-dontwarn androidx.window.sidecar.**
