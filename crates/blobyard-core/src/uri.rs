use crate::Slug;
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU64;
use std::str::FromStr;

const SCHEME: &str = "blobyard://";
const MAX_URI_BYTES: usize = 4_096;
const MAX_PATH_BYTES: usize = 2_048;
const SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// A validated, canonical Blobyard object identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlobyardUri {
    workspace: Slug,
    project: Slug,
    logical_path: String,
    version: Option<NonZeroU64>,
}

impl BlobyardUri {
    /// Creates a canonical object identifier from already validated namespace slugs.
    ///
    /// # Errors
    ///
    /// Returns a path or length error when the decoded logical path is unsafe or oversized.
    pub fn new(
        workspace: Slug,
        project: Slug,
        logical_path: String,
        version: Option<NonZeroU64>,
    ) -> Result<Self, BlobyardUriError> {
        validate_decoded_path(&logical_path)?;
        let value = Self {
            workspace,
            project,
            logical_path,
            version,
        };
        if value.to_string().len() > MAX_URI_BYTES {
            Err(BlobyardUriError::TooLong)
        } else {
            Ok(value)
        }
    }

    /// Returns the workspace slug.
    #[must_use]
    pub fn workspace(&self) -> &str {
        self.workspace.as_str()
    }

    /// Returns the validated workspace slug.
    #[must_use]
    pub const fn workspace_slug(&self) -> &Slug {
        &self.workspace
    }

    /// Returns the project slug.
    #[must_use]
    pub fn project(&self) -> &str {
        self.project.as_str()
    }

    /// Returns the validated project slug.
    #[must_use]
    pub const fn project_slug(&self) -> &Slug {
        &self.project
    }

    /// Returns the decoded logical path.
    #[must_use]
    pub fn logical_path(&self) -> &str {
        &self.logical_path
    }

    /// Returns the immutable version selector, when present.
    #[must_use]
    pub const fn version(&self) -> Option<NonZeroU64> {
        self.version
    }
}

impl Display for BlobyardUri {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{SCHEME}{}/{}", self.workspace, self.project)?;
        for segment in self.logical_path.split('/') {
            write!(
                formatter,
                "/{}",
                utf8_percent_encode(segment, SEGMENT_ENCODE_SET)
            )?;
        }
        if let Some(version) = self.version {
            write!(formatter, "?version={version}")?;
        }
        Ok(())
    }
}

impl FromStr for BlobyardUri {
    type Err = BlobyardUriError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.len() > MAX_URI_BYTES {
            return Err(BlobyardUriError::TooLong);
        }
        let body = input
            .strip_prefix(SCHEME)
            .ok_or(BlobyardUriError::InvalidScheme)?;
        if body.contains('#') {
            return Err(BlobyardUriError::InvalidStructure);
        }
        let (location, query) = split_query(body)?;
        let (workspace, project, encoded_path) = split_location(location)?;
        let workspace = Slug::new(workspace).map_err(|_| BlobyardUriError::InvalidWorkspace)?;
        let project = Slug::new(project).map_err(|_| BlobyardUriError::InvalidProject)?;
        let logical_path = decode_path(encoded_path)?;
        let version = parse_version(query)?;
        Ok(Self {
            workspace,
            project,
            logical_path,
            version,
        })
    }
}

impl Serialize for BlobyardUri {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for BlobyardUri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

fn split_query(body: &str) -> Result<(&str, Option<&str>), BlobyardUriError> {
    if let Some((location, query)) = body.split_once('?') {
        if query.contains('?') {
            return Err(BlobyardUriError::InvalidStructure);
        }
        Ok((location, Some(query)))
    } else {
        Ok((body, None))
    }
}

fn split_location(location: &str) -> Result<(&str, &str, &str), BlobyardUriError> {
    let (workspace, remainder) = location
        .split_once('/')
        .ok_or(BlobyardUriError::InvalidStructure)?;
    let (project, path) = remainder
        .split_once('/')
        .ok_or(BlobyardUriError::InvalidStructure)?;
    if workspace.is_empty() || project.is_empty() || path.is_empty() {
        return Err(BlobyardUriError::InvalidStructure);
    }
    Ok((workspace, project, path))
}

fn decode_path(encoded_path: &str) -> Result<String, BlobyardUriError> {
    validate_percent_encoding(encoded_path)?;
    let decoded_segments = encoded_path
        .split('/')
        .map(decode_segment)
        .collect::<Result<Vec<_>, _>>()?;
    let logical_path = decoded_segments.join("/");
    validate_decoded_path(&logical_path)?;
    Ok(logical_path)
}

fn validate_decoded_path(value: &str) -> Result<(), BlobyardUriError> {
    if value.len() > MAX_PATH_BYTES {
        return Err(BlobyardUriError::TooLong);
    }
    if value.split('/').any(|segment| {
        segment.is_empty()
            || matches!(segment, "." | "..")
            || segment.contains('\\')
            || segment.chars().any(char::is_control)
    }) {
        Err(BlobyardUriError::InvalidPath)
    } else {
        Ok(())
    }
}

fn decode_segment(segment: &str) -> Result<String, BlobyardUriError> {
    let decoded = percent_decode_str(segment)
        .decode_utf8()
        .map_err(|_| BlobyardUriError::InvalidPercentEncoding)?;
    let invalid = decoded.is_empty()
        || matches!(decoded.as_ref(), "." | "..")
        || decoded.contains(['/', '\\'])
        || decoded.chars().any(char::is_control);
    if invalid {
        return Err(BlobyardUriError::InvalidPath);
    }
    Ok(decoded.into_owned())
}

fn validate_percent_encoding(value: &str) -> Result<(), BlobyardUriError> {
    let mut bytes = value.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let first = bytes.next();
            let second = bytes.next();
            if !first.is_some_and(is_hex) || !second.is_some_and(is_hex) {
                return Err(BlobyardUriError::InvalidPercentEncoding);
            }
        }
    }
    Ok(())
}

const fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn parse_version(query: Option<&str>) -> Result<Option<NonZeroU64>, BlobyardUriError> {
    let Some(query) = query else {
        return Ok(None);
    };
    let raw = query
        .strip_prefix("version=")
        .ok_or(BlobyardUriError::InvalidVersion)?;
    if raw.is_empty() || raw.contains('&') {
        return Err(BlobyardUriError::InvalidVersion);
    }
    let version = raw
        .parse::<u64>()
        .ok()
        .and_then(NonZeroU64::new)
        .ok_or(BlobyardUriError::InvalidVersion)?;
    Ok(Some(version))
}

/// The reason a Blobyard URI failed validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlobyardUriError {
    /// The URI exceeds a supported size limit.
    TooLong,
    /// The URI does not use the `blobyard://` scheme.
    InvalidScheme,
    /// Required URI segments or delimiters are malformed.
    InvalidStructure,
    /// The workspace slug is invalid.
    InvalidWorkspace,
    /// The project slug is invalid.
    InvalidProject,
    /// The logical path is empty, unsafe, or ambiguous.
    InvalidPath,
    /// A percent escape or decoded UTF-8 sequence is invalid.
    InvalidPercentEncoding,
    /// The immutable version selector is invalid.
    InvalidVersion,
}

impl Display for BlobyardUriError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooLong => "Blobyard URI exceeds its length limit",
            Self::InvalidScheme => "Blobyard URI must start with blobyard://",
            Self::InvalidStructure => "Blobyard URI must include workspace, project, and path",
            Self::InvalidWorkspace => "Blobyard URI has an invalid workspace slug",
            Self::InvalidProject => "Blobyard URI has an invalid project slug",
            Self::InvalidPath => "Blobyard URI has an unsafe or ambiguous logical path",
            Self::InvalidPercentEncoding => "Blobyard URI has invalid percent encoding",
            Self::InvalidVersion => "Blobyard URI version must be a positive integer",
        })
    }
}

impl Error for BlobyardUriError {}
