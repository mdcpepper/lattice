//! API token formatting, parsing, and HMAC input construction.

use std::{fmt, str::FromStr};

use rand::{RngCore, rngs::OsRng};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::domain::tenants::records::TenantUuid;

/// API token identifier prefix.
pub const API_TOKEN_PREFIX: &str = "lt";

/// Number of secret bytes encoded in a token.
pub const API_TOKEN_SECRET_BYTES: usize = 32;

const API_TOKEN_SECRET_HEX_CHARS: usize = API_TOKEN_SECRET_BYTES * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiTokenVersion {
    V1,
}

impl ApiTokenVersion {
    #[must_use]
    pub const fn as_i16(self) -> i16 {
        match self {
            Self::V1 => 1,
        }
    }

    #[must_use]
    pub const fn segment(self) -> &'static str {
        match self {
            Self::V1 => "v1",
        }
    }
}

impl TryFrom<i16> for ApiTokenVersion {
    type Error = ApiTokenError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::V1),
            _ => Err(ApiTokenError::UnsupportedVersion),
        }
    }
}

impl From<ApiTokenVersion> for i16 {
    fn from(value: ApiTokenVersion) -> Self {
        value.as_i16()
    }
}

impl FromStr for ApiTokenVersion {
    type Err = ApiTokenError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "v1" => Ok(Self::V1),
            _ => Err(ApiTokenError::UnsupportedVersion),
        }
    }
}

#[derive(Clone)]
pub struct ApiTokenSecret {
    bytes: [u8; API_TOKEN_SECRET_BYTES],
}

impl ApiTokenSecret {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; API_TOKEN_SECRET_BYTES]) -> Self {
        Self { bytes }
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; API_TOKEN_SECRET_BYTES] {
        &self.bytes
    }
}

impl fmt::Debug for ApiTokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiTokenSecret(**redacted**)")?;
        Ok(())
    }
}

impl Drop for ApiTokenSecret {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

#[derive(Debug, Clone)]
pub struct ParsedApiToken {
    pub token_uuid: Uuid,
    pub version: ApiTokenVersion,
    pub secret: ApiTokenSecret,
}

#[derive(Debug, Error)]
pub enum ApiTokenError {
    #[error("api token format is invalid")]
    InvalidFormat,

    #[error("api token uses an unsupported version")]
    UnsupportedVersion,

    #[error("api token secret encoding is invalid")]
    InvalidSecretEncoding,
}

#[must_use]
pub fn generate_api_token_secret() -> ApiTokenSecret {
    let mut secret = [0_u8; API_TOKEN_SECRET_BYTES];

    OsRng.fill_bytes(&mut secret);

    ApiTokenSecret::from_bytes(secret)
}

#[must_use]
pub fn format_api_token(
    token_uuid: Uuid,
    version: ApiTokenVersion,
    secret: &ApiTokenSecret,
) -> String {
    format!(
        "{API_TOKEN_PREFIX}_{}_{}.{}",
        version.segment(),
        token_uuid.simple(),
        encode_secret_hex(secret.as_bytes())
    )
}

pub fn parse_api_token(token: &str) -> Result<ParsedApiToken, ApiTokenError> {
    let (prefix_and_id, secret_hex) = token.split_once('.').ok_or(ApiTokenError::InvalidFormat)?;

    let mut id_parts = prefix_and_id.splitn(3, '_');

    let prefix = id_parts.next().ok_or(ApiTokenError::InvalidFormat)?;
    let version_segment = id_parts.next().ok_or(ApiTokenError::InvalidFormat)?;
    let token_uuid_segment = id_parts.next().ok_or(ApiTokenError::InvalidFormat)?;

    if prefix != API_TOKEN_PREFIX {
        return Err(ApiTokenError::InvalidFormat);
    }

    let version = ApiTokenVersion::from_str(version_segment)?;

    let token_uuid =
        Uuid::try_parse(token_uuid_segment).map_err(|_| ApiTokenError::InvalidFormat)?;

    let secret = decode_secret_hex(secret_hex).ok_or(ApiTokenError::InvalidSecretEncoding)?;

    Ok(ParsedApiToken {
        token_uuid,
        version,
        secret: ApiTokenSecret::from_bytes(secret),
    })
}

/// Build the canonical HMAC input bytes for a token.
///
/// Format: `{token_uuid_hex}:{version_i16_decimal}:{tenant_uuid_hex}:{secret_hex}`
#[must_use]
pub fn build_verifier_input(
    token_uuid: &Uuid,
    version: ApiTokenVersion,
    tenant_uuid: &TenantUuid,
    secret: &ApiTokenSecret,
) -> Vec<u8> {
    let input = format!(
        "{}:{}:{}:{}",
        token_uuid.simple(),
        version.as_i16(),
        tenant_uuid.into_uuid().simple(),
        encode_secret_hex(secret.as_bytes()),
    );

    input.into_bytes()
}

fn encode_secret_hex(secret: &[u8; API_TOKEN_SECRET_BYTES]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut encoded = String::with_capacity(API_TOKEN_SECRET_HEX_CHARS);

    for byte in secret {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }

    encoded
}

fn decode_secret_hex(secret_hex: &str) -> Option<[u8; API_TOKEN_SECRET_BYTES]> {
    if secret_hex.len() != API_TOKEN_SECRET_HEX_CHARS {
        return None;
    }

    let mut secret = [0_u8; API_TOKEN_SECRET_BYTES];
    let secret_bytes = secret_hex.as_bytes();

    for (index, byte) in secret.iter_mut().enumerate() {
        let hi = decode_hex_nibble(secret_bytes[index * 2])?;
        let lo = decode_hex_nibble(secret_bytes[(index * 2) + 1])?;

        *byte = (hi << 4) | lo;
    }

    Some(secret)
}

fn decode_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format_round_trip() {
        let token_uuid = Uuid::nil();
        let secret = ApiTokenSecret::from_bytes([0xAB; API_TOKEN_SECRET_BYTES]);
        let token = format_api_token(token_uuid, ApiTokenVersion::V1, &secret);
        let parsed = parse_api_token(&token).expect("token should parse");

        assert_eq!(parsed.token_uuid, token_uuid);
        assert_eq!(parsed.version, ApiTokenVersion::V1);
        assert_eq!(parsed.secret.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn parse_rejects_invalid_prefix() {
        assert!(parse_api_token("nope_v1_00000000-0000-0000-0000-000000000000.aa").is_err());
    }

    #[test]
    fn build_verifier_input_is_deterministic() {
        let token_uuid = Uuid::nil();
        let tenant_uuid = TenantUuid::from_uuid(Uuid::nil());
        let secret = ApiTokenSecret::from_bytes([0xCD; API_TOKEN_SECRET_BYTES]);

        let input1 = build_verifier_input(&token_uuid, ApiTokenVersion::V1, &tenant_uuid, &secret);
        let input2 = build_verifier_input(&token_uuid, ApiTokenVersion::V1, &tenant_uuid, &secret);

        assert_eq!(input1, input2, "verifier input must be deterministic");
        assert!(!input1.is_empty(), "verifier input must not be empty");
    }
}
