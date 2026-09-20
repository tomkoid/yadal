use tracing_subscriber::prelude::*;

pub fn configure() {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_level(true);

    let filter_layer = tracing_subscriber::filter::EnvFilter::from_default_env()
        .add_directive("tidlers=debug".parse().unwrap());

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
}
