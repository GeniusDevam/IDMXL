mod adaptive;
pub mod application;
pub mod browser;
pub mod capture;
pub mod engine;
pub mod everyday;
mod fault;
pub mod management;
pub mod manifests;
pub mod media;
mod media_helper;
pub mod model;
pub mod network;
pub mod platform;
pub mod scheduler;
mod small_http;
pub mod store;
pub mod transport;
pub mod updates;
pub mod workflows;

pub use engine::{Engine, RunOptions};
pub use model::{AddJob, JobPolicy, JobView};
pub use store::Store;
pub use workflows::{FolderSettings, LinkBatch, QueueWindow};

pub fn capabilities() -> serde_json::Value {
    let version = curl::Version::get();
    serde_json::json!({
        "app":"IDMXL","version":env!("CARGO_PKG_VERSION"),"milestone":"Everyday downloads and scoped media release candidate",
        "libcurl":version.version(),"tls":version.ssl_version(),"http2":version.feature_http2(),
        "zlib":version.libz_version(),"nghttp2":version.nghttp2_version(),"sqlite":rusqlite::version(),
        "protocols":version.protocols().collect::<Vec<_>>(),
        "credential_storage":if cfg!(windows){"Windows DPAPI"}else{"fixture build: public URLs only"},
        "extent_bytes":"1048576","maximum_connections":4,"adaptive_parallelism":true,
        "adaptive_scope":"saved desktop/queue policy and explicit CLI opt-in; uncapped conditional ranges only",
        "production_ready":false
    })
}
