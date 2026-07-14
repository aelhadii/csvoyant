//! JWT access-token encoding/decoding (HS256).

use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use shared::Role;
use uuid::Uuid;

/// Access-token claims. `sub` is the user id; `role` drives RBAC without a DB hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub role: Role,
    pub iat: i64,
    pub exp: i64,
}

/// Sign a short-lived access token for `user_id`.
pub fn encode_access(
    secret: &str,
    user_id: Uuid,
    role: Role,
    ttl: Duration,
) -> anyhow::Result<String> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id,
        role,
        iat: now.timestamp(),
        exp: (now + ttl).timestamp(),
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

/// Validate + decode an access token. Rejects expired tokens (exp is validated).
pub fn decode_access(secret: &str, token: &str) -> anyhow::Result<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_token_roundtrips_with_role() {
        let uid = Uuid::new_v4();
        let token = encode_access("secret", uid, Role::Admin, Duration::minutes(15)).unwrap();
        let claims = decode_access("secret", &token).unwrap();
        assert_eq!(claims.sub, uid);
        assert_eq!(claims.role, Role::Admin);
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let token =
            encode_access("secret", Uuid::new_v4(), Role::User, Duration::minutes(15)).unwrap();
        assert!(decode_access("other-secret", &token).is_err());
    }

    #[test]
    fn expired_token_is_rejected() {
        // jsonwebtoken's default validation allows 60s of leeway, so expire well beyond it.
        let token = encode_access(
            "secret",
            Uuid::new_v4(),
            Role::User,
            Duration::seconds(-120),
        )
        .unwrap();
        assert!(decode_access("secret", &token).is_err());
    }
}
