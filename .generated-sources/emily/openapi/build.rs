use emily_handler::{api, common};
use serde_json::json;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use utoipa::OpenApi;

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(api::handlers::check_address_handler,),
    components(schemas(
        common::BlocklistStatus,
        common::RiskSeverity,
        common::error::ErrorResponse
    ))
)]
struct ApiDoc;

fn main() {

    // Ensure that we rerun if the API changes or the build script changes.
    println!("cargo:rerun-if-changed=../../../emily/handler/common/mod.rs");
    println!("cargo:rerun-if-changed=../../../emily/handler/common/handlers.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let mut api_doc = ApiDoc::openapi();
    let new_extensions: HashMap<String, serde_json::Value> = new_operation_extensions();

    // Add AWS extension to openapi specification so AWS CDK can attach the appropriate lambda endpoint.
    api_doc.paths.paths.iter_mut()
        .flat_map(|(_, path_item)| path_item.operations.iter_mut())
        .for_each(|(_, operation)|
            operation.extensions
                .get_or_insert(Default::default())
                .extend(new_extensions.clone()));

    // Generate string for api doc.
    let spec_json = api_doc
        .to_pretty_json()
        .expect("Failed to serialize OpenAPI spec");

    // Open and write to file.
    let mut file =
        File::create("emily-openapi-spec.json").expect("Failed to create OpenAPI spec file");
    file.write_all(spec_json.as_bytes())
        .expect("Failed to write OpenAPI spec file");

}

/// Creates the map of the extensions to be included in each operation.
fn new_operation_extensions() -> HashMap<String, serde_json::Value> {
    let mut extensions: HashMap<String, serde_json::Value> = HashMap::new();
    extensions.insert(
        "x-amazon-apigateway-integration".to_string(),
        json!({
            "type": "aws_proxy",
            // Note that it's always meant to be POST regardless of the verb in the api spec.
            "httpMethod": "POST",
            "uri": {
                "Fn::Sub": "arn:${AWS::Partition}:apigateway:${AWS::Region}:lambda:path/2015-03-31/functions/${OperationLambda}/invocations"
            }
        })
    );
    extensions
}
