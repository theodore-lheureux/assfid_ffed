pub use tracing::{debug, error, info, warn, trace, instrument};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt::{self, format::FmtSpan}};

pub fn init() {
    
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    let is_debug = env_filter.to_string().contains("debug") || 
                   std::env::var("RUST_LOG").unwrap_or_default().contains("debug");
    
    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_timer(fmt::time::uptime())
        .with_span_events(if is_debug {
            FmtSpan::CLOSE
        } else {
            FmtSpan::NONE
        });
    
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

