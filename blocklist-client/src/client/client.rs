use crate::common::error::Error;
use crate::common::{BlocklistStatus, RiskSeverity};
use crate::config::RiskAnalysisConfig;
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use tracing::debug;
const API_BASE_PATH: &str = "/api/risk/v2/entities";

#[derive(Deserialize, Debug)]
struct RegistrationResponse {
    address: String,
}

#[derive(Deserialize, Debug)]
struct RiskResponse {
    risk: Option<String>,
}

/// Register the user address with provider to run subsequent risk checks
async fn register_address(
    client: &Client,
    config: &RiskAnalysisConfig,
    address: &str,
) -> Result<RegistrationResponse, Error> {
    let api_url = register_address_path(&config.api_url);
    let body = serde_json::json!({ "address": address });

    debug!("Beginning registration for address: {address}");

    let response = client
        .post(&api_url)
        .header("Token", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let checked_response = check_api_response(response).await?;
    checked_response
        .json::<RegistrationResponse>()
        .await
        .map_err(Error::from)
}

/// Check risk status associated with a registered address
async fn get_risk_assessment(
    client: &Client,
    config: &RiskAnalysisConfig,
    address: &str,
) -> Result<RiskSeverity, Error> {
    let api_url = risk_assessment_path(&config.api_url, address);

    debug!("Beginning risk assessment for address: {address}");

    let response = client
        .get(&api_url)
        .header("Token", &config.api_key)
        .send()
        .await?;

    let checked_response = check_api_response(response).await?;
    let resp = checked_response.json::<RiskResponse>().await?;

    match resp.risk {
        Some(risk_str) => match risk_str.as_str() {
            "Low" => Ok(RiskSeverity::Low),
            "Medium" => Ok(RiskSeverity::Medium),
            "High" => Ok(RiskSeverity::High),
            "Severe" => Ok(RiskSeverity::Severe),
            _ => Err(Error::InvalidRiskValue(risk_str.clone())),
        },
        None => Err(Error::InvalidApiResponse),
    }
}

/// Screen the provided address for blocklist status after registering it.
/// Marks the address as not accepted if it is identified as high risk.
pub async fn check_address(
    client: &Client,
    config: &RiskAnalysisConfig,
    address: &str,
) -> Result<BlocklistStatus, Error> {
    // First, register the address
    let register_response = register_address(client, config, address).await?;
    debug!("Address registered: {}", register_response.address);

    // If registration is successful, proceed to check the address
    let assessed_response = get_risk_assessment(client, config, address).await?;
    debug!(
        "Received risk assessment response: {} for address: {}",
        register_response.address, register_response.address
    );

    let blocklist_status = BlocklistStatus {
        // `is_blocklisted` is set to true if risk is Severe
        is_blocklisted: matches!(assessed_response, RiskSeverity::Severe),
        severity: assessed_response.to_string(),
        // `accept` is set to false if severity is Severe
        accept: !matches!(assessed_response, RiskSeverity::Severe),
    };

    Ok(blocklist_status)
}

/// Evaluates the HTTP response from an API request and translates HTTP status codes into application-specific errors.
async fn check_api_response(response: Response) -> Result<Response, Error> {
    match response.status() {
        StatusCode::OK | StatusCode::CREATED => Ok(response),
        StatusCode::BAD_REQUEST => Err(Error::HttpRequestError(
            response.status(),
            "Bad request - Invalid parameters or data".to_string(),
        )),
        StatusCode::FORBIDDEN => Err(Error::Unauthorized),
        StatusCode::NOT_FOUND => Err(Error::NotFound),
        StatusCode::NOT_ACCEPTABLE => Err(Error::NotAcceptable),
        StatusCode::CONFLICT => Err(Error::Conflict),
        StatusCode::INTERNAL_SERVER_ERROR => Err(Error::InternalServerError),
        StatusCode::SERVICE_UNAVAILABLE => Err(Error::ServiceUnavailable),
        StatusCode::REQUEST_TIMEOUT => Err(Error::RequestTimeout),
        status => Err(Error::HttpRequestError(
            status,
            "Unhandled status code".to_string(),
        )),
    }
}

fn register_address_path(base_url: &str) -> String {
    format!("{}{}", base_url, API_BASE_PATH)
}

fn risk_assessment_path(base_url: &str, address: &str) -> String {
    format!("{}{}/{}", base_url, API_BASE_PATH, address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::RiskSeverity::{Low, Severe};
    use mockito::{mock, server_url, Mock};

    const TEST_ADDRESS: &str = "test_address";
    const ADDRESS_REGISTRATION_BODY: &str = r#"{"address": "test_address"}"#;

    // Setup function for common client and configuration
    fn setup_client() -> (Client, RiskAnalysisConfig) {
        let client = Client::new();
        let config = RiskAnalysisConfig {
            api_url: server_url(),
            api_key: "dummy_api_key".to_string(),
        };
        (client, config)
    }

    // Helper function to setup a mock API response
    fn setup_mock(method: &str, path: &str, status: u16, body: &str) -> Mock {
        return mock(method, path)
            .with_status(status.into())
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
    }

    #[tokio::test]
    async fn test_register_address_success() {
        let _m = setup_mock("POST", API_BASE_PATH, 200, ADDRESS_REGISTRATION_BODY);
        let (client, config) = setup_client();

        let result = register_address(&client, &config, TEST_ADDRESS).await;
        assert!(result.is_ok());
        match result {
            Ok(response) => assert_eq!(response.address, TEST_ADDRESS),
            Err(e) => panic!("Expected success, got error: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_register_address_bad_request() {
        let _m = setup_mock(
            "POST",
            API_BASE_PATH,
            400,
            r#"{"message": "Bad request - Invalid parameters or data"}"#,
        );
        let (client, config) = setup_client();

        let result = register_address(&client, &config, TEST_ADDRESS).await;
        match result {
            Err(Error::HttpRequestError(code, message)) => {
                assert_eq!(code, StatusCode::BAD_REQUEST);
                assert!(message.contains("Bad request - Invalid parameters or data"));
            }
            _ => panic!("Expected HttpRequestError, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_get_risk_assessment_high_risk() {
        let _m = setup_mock(
            "GET",
            format!("{}/{}", API_BASE_PATH, TEST_ADDRESS).as_str(),
            200,
            r#"{"risk": "Severe"}"#,
        );
        let (client, config) = setup_client();

        let result = get_risk_assessment(&client, &config, TEST_ADDRESS).await;
        match result {
            Ok(severity) => assert_eq!(severity, Severe),
            Err(e) => {
                panic!("Expected RiskSeverity::Severe, got error: {:?}", e)
            }
        }
    }

    #[tokio::test]
    async fn test_get_risk_assessment_invalid_response() {
        let _m = setup_mock(
            "GET",
            format!("{}/{}", API_BASE_PATH, TEST_ADDRESS).as_str(),
            200,
            r#"{"risky": "Severe"}"#,
        );
        let (client, config) = setup_client();

        let result = get_risk_assessment(&client, &config, TEST_ADDRESS).await;
        match result {
            Ok(_) => panic!("Test failed: Expected an Error::InvalidApiResponse, but got Ok"),
            Err(e) => match e {
                Error::InvalidApiResponse => {
                    assert!(true, "Received the expected Error::InvalidApiResponse");
                }
                _ => panic!(
                    "Test failed: Expected Error::InvalidApiResponse, got {:?}",
                    e
                ),
            },
        }
    }

    #[tokio::test]
    async fn test_check_address_blocklisted_for_high_risk() {
        let _reg_mock = setup_mock("POST", API_BASE_PATH, 200, ADDRESS_REGISTRATION_BODY);
        let _risk_mock = setup_mock(
            "GET",
            format!("{}/{}", API_BASE_PATH, TEST_ADDRESS).as_str(),
            200,
            r#"{"risk": "Severe"}"#,
        );
        let (client, config) = setup_client();

        let result = check_address(&client, &config, TEST_ADDRESS).await;
        assert!(result.is_ok());
        let status = result.unwrap();
        assert!(status.is_blocklisted);
        assert_eq!(status.severity, Severe.to_string());
        assert!(!status.accept);
    }

    #[tokio::test]
    async fn test_check_address_not_blocklisted_for_low_risk() {
        let _reg_mock = setup_mock("POST", API_BASE_PATH, 200, ADDRESS_REGISTRATION_BODY);
        let _risk_mock = setup_mock(
            "GET",
            format!("{}/{}", API_BASE_PATH, TEST_ADDRESS).as_str(),
            200,
            r#"{"risk": "Low"}"#,
        );
        let (client, config) = setup_client();

        let result = check_address(&client, &config, TEST_ADDRESS).await;
        assert!(result.is_ok());
        let status = result.unwrap();
        assert!(!status.is_blocklisted);
        assert_eq!(status.severity, Low.to_string());
        assert!(status.accept);
    }

    #[tokio::test]
    async fn test_check_address_registration_fails() {
        let _reg_mock = setup_mock(
            "POST",
            API_BASE_PATH,
            400,
            r#"{"message": "Invalid address"}"#,
        );
        let (client, config) = setup_client();

        let result = check_address(&client, &config, TEST_ADDRESS).await;
        assert!(result.is_err());
        match result {
            Err(Error::HttpRequestError(code, _)) => assert_eq!(code, StatusCode::BAD_REQUEST),
            _ => panic!("Expected HttpRequestError for bad registration"),
        }
    }

    #[tokio::test]
    async fn test_check_address_risk_assessment_fails() {
        let _reg_mock = setup_mock("POST", API_BASE_PATH, 200, ADDRESS_REGISTRATION_BODY);
        let _risk_mock = setup_mock(
            "GET",
            format!("{}/{}", API_BASE_PATH, TEST_ADDRESS).as_str(),
            500,
            r#"{}"#,
        );
        let (client, config) = setup_client();

        let result = check_address(&client, &config, TEST_ADDRESS).await;
        assert!(result.is_err());
        match result {
            Err(Error::InternalServerError) => {
                assert!(true, "Received expected internal server error")
            }
            _ => panic!("Expected InternalServerError for failed risk assessment"),
        }
    }
}
