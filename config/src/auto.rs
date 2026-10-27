use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoU32 {
    Auto,
    Value(u32),
}

impl Serialize for AutoU32 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            AutoU32::Auto => serializer.serialize_str("auto"),
            AutoU32::Value(value) => serializer.serialize_u32(*value),
        }
    }
}

struct AutoVisitor;

impl<'de> de::Visitor<'de> for AutoVisitor {
    type Value = AutoU32;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("\"auto\" or a positive integer")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<AutoU32, E> {
        if value.eq_ignore_ascii_case("auto") {
            Ok(AutoU32::Auto)
        } else {
            Err(E::invalid_value(de::Unexpected::Str(value), &self))
        }
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<AutoU32, E> {
        match u32::try_from(value) {
            Ok(number) if number > 0 => Ok(AutoU32::Value(number)),
            _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &self)),
        }
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<AutoU32, E> {
        match u64::try_from(value) {
            Ok(unsigned) => self.visit_u64(unsigned),
            Err(_) => Err(E::invalid_value(de::Unexpected::Signed(value), &self)),
        }
    }
}

impl<'de> Deserialize<'de> for AutoU32 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(AutoVisitor)
    }
}
