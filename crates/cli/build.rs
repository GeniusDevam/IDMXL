fn main() {
    // Like the Tauri executable, depend on Windows' UCRT rather than an external VCRUNTIME DLL.
    static_vcruntime::metabuild();
}
