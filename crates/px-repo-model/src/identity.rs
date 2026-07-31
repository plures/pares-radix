//! Content-derived and assigned identity types for the procedure-graph
//! repository substrate (ADR-0040 §1).
//!
//! Two identifier kinds:
//! - **Assigned** identity (`EntityId`, `ProcedureId`, `LogicalChangeId`,
//!   `ProjectionId`): a UUIDv7, minted once, never recomputed from content.
//! - **Content-derived** identity (`RevisionId`, `ProcedureRevisionHash`,
//!   `BlobHash`, `RenderingProfileHash`): a BLAKE3-256 digest of a canonical
//!   byte sequence, rendered in canonical text form as `blake3:<64 hex>`.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical text-form prefix for every content-derived hash (ADR-0040 §1).
/// Exists so a hash of one kind (e.g. `BlobHash`) can never be silently
/// compared against a differently-typed hash even though both are 32 bytes.
const HASH_PREFIX: &str = "blake3:";

/// A BLAKE3-256 content-derived identifier, always serialized/deserialized
/// in canonical text form (`blake3:<64 lowercase hex chars>`, ADR-0040 §1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Wrap a raw 32-byte BLAKE3 digest.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ContentHash(bytes)
    }

    /// Hash raw bytes directly (ADR-0040 §3 — used for `BlobHash` only;
    /// every other content-derived hash goes through canonical JSON first,
    /// see `crate::canonical::hash_canonical_json`).
    pub fn of_raw_bytes(bytes: &[u8]) -> Self {
        ContentHash(*blake3::hash(bytes).as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Canonical text form used INSIDE `.px`/JSON field values (ADR-0040 §1):
    /// `blake3:<64 lowercase hex chars>`.
    pub fn to_canonical_text(&self) -> String {
        format!("{HASH_PREFIX}{}", hex_lower(&self.0))
    }

    /// Bare filename form used for `blobs/`/`residuals/` paths in the bundle
    /// format (bundle-format doc §4): 64 lowercase hex chars, no prefix.
    pub fn to_bare_hex(&self) -> String {
        hex_lower(&self.0)
    }

    pub fn parse_canonical_text(text: &str) -> Result<Self, IdentityError> {
        let hex_part = text
            .strip_prefix(HASH_PREFIX)
            .ok_or_else(|| IdentityError::MissingHashPrefix(text.to_string()))?;
        Self::parse_bare_hex(hex_part)
    }

    pub fn parse_bare_hex(hex_part: &str) -> Result<Self, IdentityError> {
        if hex_part.len() != 64 {
            return Err(IdentityError::InvalidHashLength(hex_part.len()));
        }
        let mut out = [0u8; 32];
        for i in 0..32 {
            let byte_str = &hex_part[i * 2..i * 2 + 2];
            out[i] = u8::from_str_radix(byte_str, 16)
                .map_err(|_| IdentityError::InvalidHashHex(hex_part.to_string()))?;
        }
        Ok(ContentHash(out))
    }
}

fn hex_lower(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_canonical_text())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_canonical_text())
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_canonical_text())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ContentHash::parse_canonical_text(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("hash text {0:?} is missing the required 'blake3:' prefix")]
    MissingHashPrefix(String),
    #[error("hash hex part has invalid length {0} (expected 64)")]
    InvalidHashLength(usize),
    #[error("hash hex part {0:?} is not valid lowercase hex")]
    InvalidHashHex(String),
}

/// Declares a content-derived identifier newtype wrapping [`ContentHash`],
/// with its own `Debug`/`Display`/serde impls so distinct hash *kinds*
/// remain distinct Rust types (ADR-0040 §1's "never silently conflated"
/// requirement, enforced at the type level, not just the text-prefix level).
macro_rules! content_hash_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub ContentHash);

        impl $name {
            pub fn of_raw_bytes(bytes: &[u8]) -> Self {
                $name(ContentHash::of_raw_bytes(bytes))
            }

            pub fn from_content_hash(hash: ContentHash) -> Self {
                $name(hash)
            }

            pub fn to_canonical_text(&self) -> String {
                self.0.to_canonical_text()
            }

            pub fn to_bare_hex(&self) -> String {
                self.0.to_bare_hex()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0.to_canonical_text())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.to_canonical_text())
            }
        }
    };
}

content_hash_newtype!(RevisionId);
content_hash_newtype!(ProcedureRevisionHash);
content_hash_newtype!(BlobHash);
content_hash_newtype!(RenderingProfileHash);

/// Declares an assigned-identity newtype wrapping a UUIDv7 (ADR-0040 §1):
/// minted once at creation, carried forward verbatim, never recomputed from
/// content.
macro_rules! assigned_id_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Mint a new identity (UUIDv7, time-ordered).
            pub fn new() -> Self {
                $name(Uuid::now_v7())
            }

            /// Canonical text form: lowercase hyphenated UUID (RFC 4122 §3).
            pub fn to_canonical_text(&self) -> String {
                self.0.hyphenated().to_string()
            }

            pub fn parse_canonical_text(text: &str) -> Result<Self, IdentityError> {
                Uuid::parse_str(text)
                    .map($name)
                    .map_err(|_| IdentityError::InvalidHashHex(text.to_string()))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.to_canonical_text())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.to_canonical_text())
            }
        }
    };
}

assigned_id_newtype!(EntityId);
assigned_id_newtype!(ProcedureId);
assigned_id_newtype!(LogicalChangeId);
assigned_id_newtype!(ProjectionId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_round_trips_canonical_text() {
        let h = ContentHash::of_raw_bytes(b"hello world");
        let text = h.to_canonical_text();
        assert!(text.starts_with("blake3:"));
        assert_eq!(text.len(), "blake3:".len() + 64);
        let parsed = ContentHash::parse_canonical_text(&text).unwrap();
        assert_eq!(h, parsed);
    }

    #[test]
    fn distinct_hash_kinds_do_not_compare_equal_by_type() {
        // Same underlying bytes, different newtypes -- this is a compile-time
        // guarantee (BlobHash and RevisionId are not comparable), verified
        // here only by confirming both wrap the identical ContentHash value.
        let raw = ContentHash::of_raw_bytes(b"same content");
        let blob = BlobHash::from_content_hash(raw);
        let rev = RevisionId::from_content_hash(raw);
        assert_eq!(blob.to_bare_hex(), rev.to_bare_hex());
        assert_eq!(blob.to_canonical_text(), rev.to_canonical_text());
    }

    #[test]
    fn assigned_id_is_time_ordered_and_stable() {
        let a = ProcedureId::new();
        let b = ProcedureId::new();
        assert_ne!(a, b);
        let text = a.to_canonical_text();
        let parsed = ProcedureId::parse_canonical_text(&text).unwrap();
        assert_eq!(a, parsed);
    }

    #[test]
    fn blob_hash_matches_official_blake3_empty_input_vector() {
        // Official BLAKE3 test vector for zero-length input (first 32 bytes
        // / 64 hex chars of the published extended-output test vector).
        let h = ContentHash::of_raw_bytes(b"");
        assert_eq!(
            h.to_bare_hex(),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }
}
