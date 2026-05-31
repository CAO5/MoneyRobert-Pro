use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_logging(debug: bool) {
    let env_filter = if debug {
        tracing_subscriber::EnvFilter::new("moneyrobert_rs=debug,tower_http=debug,axum=debug")
    } else {
        tracing_subscriber::EnvFilter::new("moneyrobert_rs=info,tower_http=info")
    };

    let format = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_file(debug)
        .with_line_number(debug);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(format)
        .init();
}
