use crate::error::AuthError;
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub session_id: String,
    pub iat: i64,
}

pub fn validate_jwt_token(
    provided_token: Option<&str>,
    jwt_secret: &str,
    expected_session_id: &str,
) -> Result<Claims, AuthError> {
    let token = provided_token.ok_or(AuthError::MissingToken)?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;
    validation.required_spec_claims.clear();

    let decoding_key = DecodingKey::from_secret(jwt_secret.as_bytes());

    let token_data = decode::<Claims>(token, &decoding_key, &validation)
        .map_err(|e| {
            warn!(error = %e, "JWT validation failed");
            AuthError::JwtError(e.to_string())
        })?;

    if token_data.claims.session_id != expected_session_id {
        warn!(
            token_session = %token_data.claims.session_id,
            url_session = %expected_session_id,
            "Session ID mismatch"
        );
        return Err(AuthError::InvalidToken);
    }

    debug!(
        session_id = %expected_session_id,
        iat = token_data.claims.iat,
        "JWT validation successful"
    );

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_token(session_id: &str, secret: &str) -> String {
        let claims = Claims {
            session_id: session_id.to_string(),
            iat: 1234567890,
        };

        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_valid_jwt() {
        let secret = "test-secret-at-least-32-bytes-long-for-security";
        let session_id = "test-session-123";
        let token = create_test_token(session_id, secret);

        let result = validate_jwt_token(Some(&token), secret, session_id);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().session_id, session_id);
    }

    #[test]
    fn test_missing_token() {
        let result = validate_jwt_token(None, "secret", "session");
        assert!(matches!(result, Err(AuthError::MissingToken)));
    }

    #[test]
    fn test_wrong_secret() {
        let token = create_test_token("session", "secret1");
        let result = validate_jwt_token(Some(&token), "secret2", "session");
        assert!(matches!(result, Err(AuthError::JwtError(_))));
    }

    #[test]
    fn test_session_id_mismatch() {
        let secret = "test-secret-at-least-32-bytes-long-for-security";
        let token = create_test_token("session-a", secret);
        let result = validate_jwt_token(Some(&token), secret, "session-b");
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn test_malformed_token() {
        let result = validate_jwt_token(Some("not-a-jwt"), "secret", "session");
        assert!(matches!(result, Err(AuthError::JwtError(_))));
    }
}
