pub mod engine;
pub mod model;
pub mod platform;
pub mod store;
pub mod transport;

pub use engine::{Engine, RunOptions};
pub use model::{AddJob, JobView};
pub use store::Store;

pub fn capabilities() -> serde_json::Value {
    let version = curl::Version::get();
    serde_json::json!({
        "app":"IDMXL","version":env!("CARGO_PKG_VERSION"),"milestone":"G0/G1 development",
        "libcurl":version.version(),"tls":version.ssl_version(),"http2":version.feature_http2(),
        "protocols":version.protocols().collect::<Vec<_>>(),
        "credential_storage":if cfg!(windows){"Windows DPAPI"}else{"fixture build: public URLs only"},
        "extent_bytes":"1048576","maximum_connections":4,"adaptive_parallelism":false,
        "production_ready":false
    })
}
