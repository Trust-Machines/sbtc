//! A module for setting up metrics in the APP
//!

use std::net::SocketAddr;

use metrics_exporter_prometheus::PrometheusBuilder;

/// The buckets used for metric histograms
const METRIC_BUCKETS: [f64; 9] = [1e-4, 1e-3, 1e-2, 0.1, 0.5, 1.0, 5.0, 20.0, f64::INFINITY];

/// The quantiles to use when rendering historgrams
const METRIC_QUANTILES: [f64; 8] = [0.0, 0.25, 0.5, 0.75, 0.9, 0.95, 0.99, 1.0];

/// The metric for the total number of submitted transactions.
pub const TRANSACTIONS_SUBMITTED_TOTAL: &str = "transactions_submitted_total";

/// The metric for the total number of observed bitcoin or stacks blocks.
/// We use a label to distinguish between the two. Note that this only
/// includes bitcoin blocks observered over the ZeroMQ interface and stacks
/// blocks observed from the event observer.
pub const BLOCKS_OBSERVED_TOTAL: &str = "blocks_observed_total";

/// The number of deposit requests processed from Emily. This includes
/// duplicates.
pub const DEPOSIT_REQUESTS_TOTAL: &str = "deposit-requests-total";

/// The total number of signing rounds that have completed successfully.
/// This includes WSTS and "regular" multi-sig signing rounds on stacks. We
/// use a label to distringuish between the two.
pub const SIGNING_ROUNDS_COMPLETED_TOTAL: &str = "signing_rounds_completed_total";

/// The amount of time it took to complete a signing round in seconds. This
/// includes WSTS and "regular" multi-sig signing rounds on stacks. We use
/// a label to distringuish between the two.
pub const SIGNING_ROUND_DURATION_SECONDS: &str = "signing_round_duration_seconds";

/// The total number of tenures that this signer has served as coordinator.
pub const COORDINATOR_TENURES_TOTAL: &str = "coordinator_tenures_total";

/// Set up a prometheus exporter for metrics.
pub fn setup_metrics(prometheus_exporter_endpoint: Option<SocketAddr>) {
    if let Some(addr) = prometheus_exporter_endpoint {
        PrometheusBuilder::new()
            .with_http_listener(addr)
            .add_global_label("app", crate::PACKAGE_NAME)
            .set_buckets(&METRIC_BUCKETS)
            .expect("received an empty slice of metric buckets")
            .set_quantiles(&METRIC_QUANTILES)
            .expect("received an empty slice of metric quantiles")
            .install()
            .expect("could not install the prometheus server");
    }

    metrics::gauge!(
        "build_info",
        "rust_version" => crate::RUSTC_VERSION,
        "revision" => crate::GIT_COMMIT,
        "arch" => crate::TARGET_ARCH,
        "env_abi" => crate::TARGET_ENV_ABI,
    )
    .set(1.0);
}
