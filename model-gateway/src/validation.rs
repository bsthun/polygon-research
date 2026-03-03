//! Validation module for API key validation

use crate::config::Config;
use hyper::StatusCode;

/// Validates the API key from the Authorization header.
///
/// # Arguments
/// * `authorization` - The Authorization header value
/// * `config` - The application configuration
///
/// # Returns
/// * `Ok(())` if the API key is valid
/// * `Err(StatusCode::UNAUTHORIZED)` if the API key is missing or invalid
pub fn validate_api_key(authorization: &str, config: &Config) -> Result<(), StatusCode> {
    let token = authorization
        .strip_prefix("Bearer ")
        .or_else(|| authorization.strip_prefix("bearer "))
        .unwrap_or(authorization);

    if token.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if token != config.api_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Upstream};

    fn test_config() -> Config {
        Config {
            listen: ":8000".to_string(),
            api_key: "test-key".to_string(),
            upstreams: vec![Upstream {
                name: "test".to_string(),
                openai_endpoint: "https://api.example.com/v1".to_string(),
                anthropic_endpoint: "https://api.example.com/anthropic".to_string(),
                key: "upstream-key".to_string(),
            }],
        }
    }

    #[test]
    fn test_valid_api_key() {
        let config = test_config();
        let result = validate_api_key("Bearer test-key", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_api_key() {
        let config = test_config();
        let result = validate_api_key("Bearer wrong-key", &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_api_key() {
        let config = test_config();
        let result = validate_api_key("", &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_case_insensitive_bearer() {
        let config = test_config();
        let result = validate_api_key("bearer test-key", &config);
        assert!(result.is_ok());
    }
}
