use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Canonical lower-case UUID text. Distinct field meanings remain in typed messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct Id(String);

impl TryFrom<String> for Id {
    type Error = &'static str;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.len() != 36
            || text.bytes().enumerate().any(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c != b'-'
                } else {
                    !c.is_ascii_digit() && !(b'a'..=b'f').contains(&c)
                }
            })
        {
            return Err("expected canonical lower-case UUID");
        }
        Ok(Self(text))
    }
}
impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.0
    }
}

/// JSON integer precision is preserved across Rust, Python and JavaScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DecimalU64(#[schemars(with = "String")] pub u64);
impl TryFrom<String> for DecimalU64 {
    type Error = &'static str;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.is_empty()
            || (text.len() > 1 && text.starts_with('0'))
            || !text.bytes().all(|c| c.is_ascii_digit())
        {
            return Err("expected canonical decimal u64 string");
        }
        text.parse().map(Self).map_err(|_| "u64 overflow")
    }
}
impl From<DecimalU64> for String {
    fn from(value: DecimalU64) -> Self {
        value.0.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256(String);
impl TryFrom<String> for Sha256 {
    type Error = &'static str;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.len() != 64
            || !text
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("expected lower-case SHA256 hex");
        }
        Ok(Self(text))
    }
}
impl From<Sha256> for String {
    fn from(value: Sha256) -> Self {
        value.0
    }
}
