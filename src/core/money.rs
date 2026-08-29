use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
use std::sync::LazyLock;

// Regex strictly enforcing integer paisa or up to 2 decimal places (§0 R1)
static MONEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{1,14}(\.\d{1,2})?$").expect("valid regex"));

pub static PIN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4,6}$").expect("valid pin regex"));

// Maximum allowable transfer/holding cap (৳100M = 10,000,000,000 paisa, §6 C18)
pub const MAX_PAISA_AMOUNT: i64 = 10_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Paisa(pub i64);

impl Paisa {
    // Parse decimal string (e.g. "1200.50" or "120000") into Paisa
    pub fn parse_from_str(s: &str) -> Result<Self, MoneyParseError> {
        let trimmed = s.trim();
        if !MONEY_REGEX.is_match(trimmed) {
            return Err(MoneyParseError::InvalidFormat);
        }

        let paisa_val = if let Some((taka_part, poisha_part)) = trimmed.split_once('.') {
            let taka: i64 = taka_part.parse().map_err(|_| MoneyParseError::Overflow)?;
            let poisha: i64 = match poisha_part.len() {
                1 => format!("{}0", poisha_part)
                    .parse()
                    .map_err(|_| MoneyParseError::InvalidFormat)?,
                2 => poisha_part
                    .parse()
                    .map_err(|_| MoneyParseError::InvalidFormat)?,
                _ => return Err(MoneyParseError::TooManyDecimalPlaces),
            };
            taka.checked_mul(100)
                .and_then(|v| v.checked_add(poisha))
                .ok_or(MoneyParseError::Overflow)?
        } else {
            trimmed
                .parse::<i64>()
                .map_err(|_| MoneyParseError::Overflow)?
        };

        if paisa_val > MAX_PAISA_AMOUNT {
            return Err(MoneyParseError::ExceedsMaxCap);
        }

        Ok(Paisa(paisa_val))
    }

    // Require amount to be strictly positive (> 0 paisa)
    pub fn parse_positive_from_str(s: &str) -> Result<Self, MoneyParseError> {
        let money = Self::parse_from_str(s)?;
        if money.0 <= 0 {
            return Err(MoneyParseError::NonPositive);
        }
        Ok(money)
    }

    pub fn as_paisa(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Paisa)
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Paisa)
    }
}

impl fmt::Display for Paisa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Paisa {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Paisa {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PaisaVisitor;

        impl<'de> de::Visitor<'de> for PaisaVisitor {
            type Value = Paisa;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("a decimal string representing amount in paisa (e.g. \"120000\")")
            }

            fn visit_str<E>(self, value: &str) -> Result<Paisa, E>
            where
                E: de::Error,
            {
                Paisa::parse_from_str(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(PaisaVisitor)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyParseError {
    #[error("invalid money format: must be decimal string matching ^\\d{{1,14}}(\\.\\d{{1,2}})?$")]
    InvalidFormat,
    #[error("money value cannot have more than 2 decimal places")]
    TooManyDecimalPlaces,
    #[error("money value must be strictly positive")]
    NonPositive,
    #[error("money value overflows maximum supported limit")]
    Overflow,
    #[error("money value exceeds maximum allowable cap")]
    ExceedsMaxCap,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_paisa_parsing() {
        let p1 = Paisa::parse_from_str("10000000").unwrap();
        assert_eq!(p1.as_paisa(), 10000000);

        let p2 = Paisa::parse_from_str("12.50").unwrap();
        assert_eq!(p2.as_paisa(), 1250);

        let p3 = Paisa::parse_from_str("0.05").unwrap();
        assert_eq!(p3.as_paisa(), 5);
    }

    #[test]
    fn test_c02_rejects_negative_and_excess_decimals() {
        assert_eq!(
            Paisa::parse_from_str("-100"),
            Err(MoneyParseError::InvalidFormat)
        );
        assert_eq!(
            Paisa::parse_from_str("12.345"),
            Err(MoneyParseError::InvalidFormat)
        );
        assert_eq!(
            Paisa::parse_positive_from_str("0"),
            Err(MoneyParseError::NonPositive)
        );
    }

    #[test]
    fn test_c13_rejects_junk_and_floats() {
        assert_eq!(
            Paisa::parse_from_str("abc"),
            Err(MoneyParseError::InvalidFormat)
        );
        assert_eq!(
            Paisa::parse_from_str("12.3.4"),
            Err(MoneyParseError::InvalidFormat)
        );
        assert_eq!(
            Paisa::parse_from_str("12e5"),
            Err(MoneyParseError::InvalidFormat)
        );
    }

    #[test]
    fn test_c18_rejects_absurd_amount_cap() {
        let absurd = "9999999999999999";
        assert!(Paisa::parse_from_str(absurd).is_err());
    }

    #[test]
    fn test_serde_json_string_roundtrip() {
        let money = Paisa(120000);
        let json = serde_json::to_string(&money).unwrap();
        assert_eq!(json, "\"120000\"");

        let deserialized: Paisa = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, money);

        // Reject floating point numbers in JSON
        let float_json = "1200.50";
        assert!(serde_json::from_str::<Paisa>(float_json).is_err());
    }
}
