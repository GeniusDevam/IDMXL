/// Test-only abrupt exit: skips destructors, but does not simulate hardware power loss.
pub(crate) fn point(name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("IDMXL_FAULT").is_ok_and(|value| value == name) {
        std::process::exit(86);
    }
    #[cfg(not(feature = "fault-injection"))]
    let _ = name;
}
