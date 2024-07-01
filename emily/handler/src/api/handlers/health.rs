//! Handlers for Health endpoint endpoints.

use crate::common::error::Error;
use warp::filters::path::FullPath;

/// Get health handler.
#[utoipa::path(
    get,
    operation_id = "checkHealth",
    path = "/health",
    tag = "health",
    responses(
        // TODO: https://github.com/stacks-network/sbtc/issues/271
        // Add success body.
        (status = 200, description = "Successfully retrieved health data.", body = HealthData),
        (status = 400, description = "Invalid request body"),
        (status = 404, description = "Address not found"),
        (status = 405, description = "Method not allowed"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_health(
    path: FullPath,
) -> impl warp::reply::Reply {
    Error::NotImplemented(path)
}
