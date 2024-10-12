#[macro_use] extern crate tracing;

use tracing_subscriber::EnvFilter;
use std::env;
fn main() {

    if let Err(_) = env::var("RUST_LOG") {
        env::set_var("RUST_LOG", "info");
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("Hello, world!");
}
