//! Serde helpers that write u128 amounts and u64 nanosecond times as decimal strings.

use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer, T: ToString>(value: &T, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&value.to_string())
}

pub fn deserialize<'de, D: Deserializer<'de>, T: std::str::FromStr>(d: D) -> Result<T, D::Error> {
    let text = String::deserialize(d)?;
    text.parse()
        .map_err(|_| serde::de::Error::custom("expected a decimal string"))
}

pub mod opt {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer, T: ToString>(
        value: &Option<T>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(v) => s.serialize_str(&v.to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: std::str::FromStr>(
        d: D,
    ) -> Result<Option<T>, D::Error> {
        match Option::<String>::deserialize(d)? {
            Some(text) => text
                .parse()
                .map(Some)
                .map_err(|_| serde::de::Error::custom("expected a decimal string")),
            None => Ok(None),
        }
    }
}
