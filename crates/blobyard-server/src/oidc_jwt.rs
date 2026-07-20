use super::OidcVerificationError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

const MAX_TOKEN_BYTES: usize = 64 * 1_024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct Jwk {
    pub(super) alg: Option<String>,
    pub(super) e: Option<String>,
    pub(super) kid: Option<String>,
    pub(super) kty: Option<String>,
    pub(super) n: Option<String>,
    #[serde(rename = "use")]
    pub(super) public_key_use: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct JwkSet {
    keys: Vec<Jwk>,
}

impl JwkSet {
    pub(super) fn find(&self, kid: &str) -> Option<&Jwk> {
        self.keys.iter().find(|key| key.kid.as_deref() == Some(kid))
    }
}

#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
    kid: Option<String>,
}

pub(super) fn key_id(token: &str) -> Result<String, OidcVerificationError> {
    let (encoded_header, _payload, _signature) = token_parts(token)?;
    let header: JwtHeader = decode_json(encoded_header)?;
    if header.alg != "RS256" {
        return Err(OidcVerificationError::Invalid);
    }
    header.kid.ok_or(OidcVerificationError::Invalid)
}

pub(super) fn verify(token: &str, key: &Jwk) -> Result<serde_json::Value, OidcVerificationError> {
    let (encoded_header, encoded_payload, encoded_signature) = token_parts(token)?;
    let payload = decode_json(encoded_payload)?;
    let modulus = decode_value(key.n.as_deref().ok_or(OidcVerificationError::Invalid)?)?;
    let exponent = decode_value(key.e.as_deref().ok_or(OidcVerificationError::Invalid)?)?;
    let signature = decode_value(encoded_signature)?;
    let signed = format!("{encoded_header}.{encoded_payload}");
    let public_key = rsa_public_key_der(&modulus, &exponent)?;
    if webpki::aws_lc_rs::RSA_PKCS1_2048_8192_SHA256
        .verify_signature(&public_key, signed.as_bytes(), signature.as_slice())
        .is_err()
    {
        return Err(OidcVerificationError::Invalid);
    }
    Ok(payload)
}

fn token_parts(token: &str) -> Result<(&str, &str, &str), OidcVerificationError> {
    if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
        return Err(OidcVerificationError::Invalid);
    }
    let mut parts = token.split('.');
    let header = parts.next().filter(|value| !value.is_empty());
    let payload = parts.next().filter(|value| !value.is_empty());
    let signature = parts.next().filter(|value| !value.is_empty());
    if let (Some(header), Some(payload), Some(signature), None) =
        (header, payload, signature, parts.next())
    {
        Ok((header, payload, signature))
    } else {
        Err(OidcVerificationError::Invalid)
    }
}

fn decode_json<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T, OidcVerificationError> {
    let decoded = decode_value(value)?;
    serde_json::from_slice(&decoded).map_err(|_error| OidcVerificationError::Invalid)
}

fn decode_value(value: &str) -> Result<Vec<u8>, OidcVerificationError> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_error| OidcVerificationError::Invalid)
}

fn rsa_public_key_der(modulus: &[u8], exponent: &[u8]) -> Result<Vec<u8>, OidcVerificationError> {
    if !(256..=1_024).contains(&modulus.len()) || !(1..=8).contains(&exponent.len()) {
        return Err(OidcVerificationError::Invalid);
    }
    let mut integers = Vec::with_capacity(modulus.len().saturating_add(16));
    append_der_integer(&mut integers, modulus);
    append_der_integer(&mut integers, exponent);
    let mut encoded = Vec::with_capacity(integers.len().saturating_add(4));
    encoded.push(0x30);
    append_der_length(&mut encoded, integers.len());
    encoded.extend(integers);
    Ok(encoded)
}

fn append_der_integer(encoded: &mut Vec<u8>, value: &[u8]) {
    let value = value
        .iter()
        .position(|byte| *byte != 0)
        .map_or_else(|| &value[value.len() - 1..], |index| &value[index..]);
    encoded.push(0x02);
    let needs_positive_prefix = value[0] & 0x80 != 0;
    let length = value.len() + usize::from(needs_positive_prefix);
    append_der_length(encoded, length);
    if needs_positive_prefix {
        encoded.push(0);
    }
    encoded.extend(value);
}

fn append_der_length(encoded: &mut Vec<u8>, length: usize) {
    let least_significant_byte = std::mem::size_of::<usize>() - 1;
    if length < 128 {
        encoded.push(length.to_be_bytes()[least_significant_byte]);
        return;
    }
    let bytes = length.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len() - 1);
    let length_bytes = bytes.len() - first;
    encoded.push(0x80 | length_bytes.to_be_bytes()[least_significant_byte]);
    encoded.extend(&bytes[first..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: Option<String>, e: Option<String>) -> Jwk {
        Jwk {
            alg: Some("RS256".to_owned()),
            e,
            kid: Some("fixture".to_owned()),
            kty: Some("RSA".to_owned()),
            n,
            public_key_use: Some("sig".to_owned()),
        }
    }

    #[test]
    fn token_parser_rejects_each_malformed_shape_and_bound() {
        for token in ["", "a.b", ".b.c", "a..c", "a.b.", "a.b.c.d"] {
            assert_eq!(key_id(token), Err(OidcVerificationError::Invalid));
        }
        assert_eq!(
            key_id(&"a".repeat(MAX_TOKEN_BYTES + 1)),
            Err(OidcVerificationError::Invalid)
        );
        for token in ["!.e30.AA", "bm90LWpzb24.e30.AA"] {
            assert_eq!(key_id(token), Err(OidcVerificationError::Invalid));
        }
    }

    #[test]
    fn verifier_rejects_missing_and_malformed_key_material() {
        let token = "e30.e30.AA";
        let modulus = URL_SAFE_NO_PAD.encode(vec![1; 256]);
        let short_modulus = URL_SAFE_NO_PAD.encode(vec![1; 255]);
        for (candidate, candidate_key) in [
            ("invalid", key(None, None)),
            (token, key(None, Some("AQAB".to_owned()))),
            (token, key(Some(modulus.clone()), None)),
            (
                "e30.e30.!",
                key(Some(modulus.clone()), Some("AQAB".to_owned())),
            ),
            (token, key(Some(modulus.clone()), Some("AQAB".to_owned()))),
            (token, key(Some(modulus), Some("!".to_owned()))),
            (token, key(Some(short_modulus), Some("AQAB".to_owned()))),
        ] {
            assert_eq!(
                verify(candidate, &candidate_key),
                Err(OidcVerificationError::Invalid)
            );
        }
        assert!(decode_json::<serde_json::Value>("!").is_err());
        assert!(decode_json::<serde_json::Value>("bm90LWpzb24").is_err());
    }

    #[test]
    fn rsa_der_encoder_covers_integer_and_length_boundaries() {
        for (modulus, exponent) in [
            (vec![1; 255], vec![1]),
            (vec![1; 1_025], vec![1]),
            (vec![1; 256], Vec::new()),
            (vec![1; 256], vec![1; 9]),
        ] {
            assert_eq!(
                rsa_public_key_der(&modulus, &exponent),
                Err(OidcVerificationError::Invalid)
            );
        }

        for (value, expected) in [
            (vec![0, 0], vec![0x02, 0x01, 0]),
            (vec![0, 1], vec![0x02, 0x01, 1]),
            (vec![0x80], vec![0x02, 0x02, 0, 0x80]),
        ] {
            let mut encoded = Vec::new();
            append_der_integer(&mut encoded, &value);
            assert_eq!(encoded, expected);
        }

        for (length, expected) in [
            (127, vec![127]),
            (128, vec![0x81, 128]),
            (256, vec![0x82, 0x01, 0]),
            (65_536, vec![0x83, 0x01, 0, 0]),
        ] {
            let mut encoded = Vec::new();
            append_der_length(&mut encoded, length);
            assert_eq!(encoded, expected);
        }
    }
}
