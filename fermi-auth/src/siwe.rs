use chrono::Utc;
use ethers::types::Address;
use hex;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use siwe::{Message, TimeStamp, Version};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

use crate::error::AuthError;

/// Request to generate a SIWE challenge
#[derive(Debug, Deserialize)]
pub struct SiweChallenge {
    pub address: String,
}

/// Response containing the challenge message
#[derive(Debug, Serialize)]
pub struct SiweChallengeResponse {
    pub message: String,
    pub nonce: String,
}

/// Request to verify a signed SIWE message
#[derive(Debug, Deserialize)]
pub struct SiweVerify {
    pub message: String,
    pub signature: String,
}

/// Response after successful SIWE verification
#[derive(Debug, Serialize)]
pub struct SiweVerifyResponse {
    pub ethereum_address: String,
    pub ens_name: Option<String>,
}

/// Generate a cryptographically secure random nonce
fn generate_nonce() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Generate a SIWE challenge message for wallet signing
pub async fn create_challenge(
    address: String,
    domain: String,
    pool: &PgPool,
) -> Result<SiweChallengeResponse, AuthError> {
    // Validate Ethereum address format
    let _parsed_address: Address = address
        .parse()
        .map_err(|_| AuthError::InvalidEthereumAddress)?;

    // Generate nonce
    let nonce = generate_nonce();

    // Create SIWE message (EIP-4361)
    let now = OffsetDateTime::now_utc();
    let expires = now + Duration::minutes(5);

    // Parse address string to ethers::Address, then convert to [u8; 20]
    let parsed_address: Address = address
        .parse()
        .map_err(|_| AuthError::InvalidEthereumAddress)?;

    let message = Message {
        domain: domain.parse().map_err(|_| AuthError::InvalidDomain)?,
        address: parsed_address.0, // Address is a newtype around [u8; 20]
        statement: Some("Sign in to Fermi Agent Bestiary".to_string()),
        uri: format!("https://{}", domain)
            .parse()
            .map_err(|_| AuthError::InvalidDomain)?,
        version: Version::V1,
        chain_id: 1, // Ethereum mainnet
        nonce: nonce.clone(),
        issued_at: TimeStamp::from(now),
        expiration_time: Some(TimeStamp::from(expires)),
        not_before: None,
        request_id: None,
        resources: vec![],
    };

    // Store nonce in database with expiration
    // Convert time::OffsetDateTime to chrono for sqlx
    let expires_at = chrono::DateTime::from_timestamp(expires.unix_timestamp(), 0)
        .ok_or(AuthError::ConfigError)?;
    sqlx::query(
        r#"
        INSERT INTO siwe_nonces (nonce, expires_at)
        VALUES ($1, $2)
        "#,
    )
    .bind(&nonce)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(SiweChallengeResponse {
        message: message.to_string(),
        nonce,
    })
}

/// Verify a signed SIWE message and return the Ethereum address
pub async fn verify_signature(
    message_str: String,
    signature: String,
    pool: &PgPool,
) -> Result<SiweVerifyResponse, AuthError> {
    // Parse the SIWE message
    let message: Message = message_str
        .parse()
        .map_err(|_| AuthError::InvalidSignature)?;

    // Check if nonce exists and is not expired
    let nonce_record = sqlx::query_as::<_, (String, chrono::DateTime<Utc>)>(
        r#"
        SELECT nonce, expires_at
        FROM siwe_nonces
        WHERE nonce = $1
        "#,
    )
    .bind(&message.nonce)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    .ok_or(AuthError::NonceNotFound)?;

    // Check expiration
    if nonce_record.1 < Utc::now() {
        return Err(AuthError::NonceExpired);
    }

    // Verify the signature
    let signature_bytes =
        hex::decode(signature.trim_start_matches("0x")).map_err(|_| AuthError::InvalidSignature)?;

    // Verify with default options (no timestamp validation since we check expiration ourselves)
    message
        .verify(&signature_bytes, &Default::default())
        .await
        .map_err(|_| AuthError::InvalidSignature)?;

    // Delete the nonce (one-time use)
    sqlx::query(
        r#"
        DELETE FROM siwe_nonces
        WHERE nonce = $1
        "#,
    )
    .bind(&message.nonce)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    // Convert address to checksummed string
    let ethereum_address = format!("{:?}", message.address);

    // TODO: Resolve ENS name (optional enhancement)
    let ens_name = None;

    Ok(SiweVerifyResponse {
        ethereum_address,
        ens_name,
    })
}

/// Cleanup expired nonces (should be called periodically)
pub async fn cleanup_expired_nonces(pool: &PgPool) -> Result<u64, AuthError> {
    let result = sqlx::query(
        r#"
        DELETE FROM siwe_nonces
        WHERE expires_at < NOW()
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_nonce() {
        let nonce1 = generate_nonce();
        let nonce2 = generate_nonce();

        // Nonces should be 64 chars (32 bytes hex encoded)
        assert_eq!(nonce1.len(), 64);
        assert_eq!(nonce2.len(), 64);

        // Nonces should be different
        assert_ne!(nonce1, nonce2);
    }

    #[test]
    fn test_valid_ethereum_address() {
        let valid = "0x742d35Cc6634C0532925a3b844Bc454e4438f44e";
        let parsed: Result<Address, _> = valid.parse();
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_invalid_ethereum_address() {
        let invalid = "not-an-address";
        let parsed: Result<Address, _> = invalid.parse();
        assert!(parsed.is_err());
    }
}
