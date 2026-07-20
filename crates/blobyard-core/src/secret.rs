use crate::{BlobyardError, ErrorCode};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt::{self, Debug, Formatter};
use zeroize::Zeroizing;

const MAX_SECRET_BYTES: usize = 16_384;
const PREVIEW_HOST_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Fixed prefixes for credentials generated entirely inside Blob Yard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedSecretKind {
    /// API access token returned after bootstrap exchange.
    AccessToken,
    /// User-managed API token returned once at creation.
    ApiToken,
    /// Short-lived GitHub Actions machine session.
    MachineToken,
    /// One-time bootstrap authority.
    BootstrapToken,
    /// Server runtime signing secret.
    RuntimeSecret,
    /// Download capability.
    DownloadCapability,
    /// Public share capability.
    ShareCapability,
    /// Public upload-inbox capability.
    InboxCapability,
    /// Upload capability.
    UploadCapability,
}

impl GeneratedSecretKind {
    const fn prefix(self) -> &'static str {
        match self {
            Self::AccessToken => "bya",
            Self::ApiToken => "byd_pat",
            Self::MachineToken => "byd_ci",
            Self::BootstrapToken => "byb",
            Self::RuntimeSecret => "bys",
            Self::DownloadCapability => "byd",
            Self::ShareCapability => "bysh",
            Self::InboxCapability => "byin",
            Self::UploadCapability => "byu",
        }
    }
}

/// A zeroizing string whose debug representation never exposes its value.
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    /// Validates and wraps a nonempty single-line secret.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_REQUEST` for empty, overlong, or control-bearing input.
    pub fn new(value: impl Into<String>) -> Result<Self, BlobyardError> {
        let value = value.into();
        let invalid = value.is_empty()
            || value.len() > MAX_SECRET_BYTES
            || value.chars().any(char::is_control);
        if invalid {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "The credential value isn't valid. Request a new credential and try again.",
            ));
        }
        Ok(Self(Zeroizing::new(value)))
    }

    /// Wraps a value whose owning Core type has already proven the secret invariants.
    pub(crate) fn from_validated(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// Builds a fixed-shape Blob Yard credential from generated entropy.
    ///
    /// The caller supplies a closed credential kind and 256 bits of generated or derived data, so
    /// the resulting value is always nonempty, bounded, ASCII, and free of control characters.
    #[must_use]
    pub fn from_generated_entropy(kind: GeneratedSecretKind, entropy: [u8; 32]) -> Self {
        Self(Zeroizing::new(format!(
            "{}_{}",
            kind.prefix(),
            crate::hex_digest(&entropy)
        )))
    }

    /// Builds the DNS-safe 256-bit capability used as an isolated preview host label.
    ///
    /// The 52-character lowercase base32 form fits in one DNS label and retains all 256 bits of
    /// supplied entropy. It intentionally has no readable prefix because a prefixed value would
    /// exceed the DNS label limit.
    #[must_use]
    pub fn from_preview_host_entropy(entropy: [u8; 32]) -> Self {
        Self(Zeroizing::new(encode_preview_host(entropy)))
    }

    /// Exposes the secret only at an explicit credential boundary.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

fn encode_preview_host(entropy: [u8; 32]) -> String {
    let mut output = String::with_capacity(52);
    let mut buffer = 0_u16;
    let mut bit_count = 0_u8;
    for byte in entropy {
        buffer = (buffer << 8) | u16::from(byte);
        bit_count += 8;
        while bit_count >= 5 {
            bit_count -= 5;
            let index = usize::from((buffer >> bit_count) & 0x1f);
            output.push(char::from(PREVIEW_HOST_ALPHABET[index]));
            buffer &= (1_u16 << bit_count).saturating_sub(1);
        }
    }
    // Exactly 256 input bits always leave one final bit after five-bit groups.
    let index = usize::from((buffer << (5 - bit_count)) & 0x1f);
    output.push(char::from(PREVIEW_HOST_ALPHABET[index]));
    output
}

impl Clone for SecretString {
    fn clone(&self) -> Self {
        Self(Zeroizing::new(self.expose_secret().to_owned()))
    }
}

impl Debug for SecretString {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        self.expose_secret() == other.expose_secret()
    }
}

impl Eq for SecretString {}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.expose_secret())
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}
