mod adaptive;
pub mod engine;
mod fault;
pub mod model;
pub mod network;
pub mod platform;
pub mod scheduler;
pub mod store;
pub mod transport;
pub mod workflows;

pub use engine::{Engine, RunOptions};
pub use model::{AddJob, JobPolicy, JobView};
pub use store::Store;
pub use workflows::{FolderSettings, LinkBatch, QueueWindow};

pub fn capabilities() -> serde_json::Value {
    let version = curl::Version::get();
    serde_json::json!({
        "app":"IDMXL","version":env!("CARGO_PKG_VERSION"),"milestone":"G0/G1 development",
        "libcurl":version.version(),"tls":version.ssl_version(),"http2":version.feature_http2(),
        "zlib":version.libz_version(),"nghttp2":version.nghttp2_version(),"sqlite":rusqlite::version(),
        "protocols":version.protocols().collect::<Vec<_>>(),
        "credential_storage":if cfg!(windows){"Windows DPAPI"}else{"fixture build: public URLs only"},
        "extent_bytes":"1048576","maximum_connections":4,"adaptive_parallelism":true,
        "adaptive_scope":"experimental CLI opt-in; uncapped conditional ranges only",
        "production_ready":false
    })
}
