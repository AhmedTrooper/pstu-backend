use crate::core::money::Paisa;
use crate::features::ai::dto::{AIIntentDto, AIParseResponse};
use regex::Regex;
use std::sync::LazyLock;

static PIN_STRIP_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:pin|পিন)\s*[:#]?\s*\d{4,6}\b").expect("valid regex"));

static LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:create|make|generate).{0,20}?(?:link|payment link).{0,30}?(\d+(?:\.\d{1,2})?)\s*(?:bdt|taka|tk|৳)?").expect("valid regex")
});

static EXPIRY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:expires|valid (?:for|in)|for)\s*(\d+)\s*(min|minute|hr|hour|day)s?")
        .expect("valid regex")
});

static TRANSFER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:send|pay|transfer)\s+(\d+(?:\.\d{1,2})?)\s*(?:bdt|taka|tk|৳)?\s*(?:to)?\s*(0\d{9,10}|account#?\d*)").expect("valid regex")
});

static REQUEST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:request|ask)\s+(\d+(?:\.\d{1,2})?)\s*(?:bdt|taka|tk|৳)?\s*(?:from)?\s*(0\d{9,10}|account#?\d*)").expect("valid regex")
});

static BALANCE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:my\s+)?(?:balance|statement)").expect("valid regex"));

static HISTORY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:history|transactions|activity)").expect("valid regex"));

// Converts BDT / Taka natural language string to Paisa (৳1 = 100 paisa, §4)
fn parse_natural_taka_to_paisa(s: &str) -> Option<Paisa> {
    let trimmed = s.trim();
    if let Some((taka_part, poisha_part)) = trimmed.split_once('.') {
        let taka: i64 = taka_part.parse().ok()?;
        let poisha_str = match poisha_part.len() {
            1 => format!("{}0", poisha_part),
            2 => poisha_part.to_string(),
            _ => return None,
        };
        let poisha: i64 = poisha_str.parse().ok()?;
        let total = taka.checked_mul(100)?.checked_add(poisha)?;
        Some(Paisa(total))
    } else {
        let taka: i64 = trimmed.parse().ok()?;
        let total = taka.checked_mul(100)?;
        Some(Paisa(total))
    }
}

#[allow(clippy::collapsible_if)]
pub fn parse_intent(text: &str) -> AIParseResponse {
    let mut flags = Vec::new();

    // Pre-step: strip PIN tokens and flag (R10, R17, C48)
    let cleaned_text = if PIN_STRIP_REGEX.is_match(text) {
        flags.push("pin_via_text_ignored".to_string());
        PIN_STRIP_REGEX.replace_all(text, "").to_string()
    } else {
        text.to_string()
    };

    let trimmed = cleaned_text.trim();

    // Check for vague relative dates (C32)
    if trimmed.to_lowercase().contains("tomorrow") || trimmed.to_lowercase().contains("next week") {
        flags.push("needs_review".to_string());
    }

    // 1. Match Link Creation (§4 Grammar)
    if let Some(caps) = LINK_REGEX.captures(trimmed) {
        if let Some(amt_match) = caps.get(1) {
            if let Some(paisa) = parse_natural_taka_to_paisa(amt_match.as_str()) {
                let mut ttl = 10800u64; // Default 3 hours (§4, C31)
                if let Some(exp_caps) = EXPIRY_REGEX.captures(trimmed) {
                    if let (Some(num_m), Some(unit_m)) = (exp_caps.get(1), exp_caps.get(2)) {
                        if let Ok(num) = num_m.as_str().parse::<u64>() {
                            let unit = unit_m.as_str().to_lowercase();
                            if unit.starts_with("min") {
                                ttl = num * 60;
                            } else if unit.starts_with("hr") || unit.starts_with("hour") {
                                ttl = num * 3600;
                            } else if unit.starts_with("day") {
                                ttl = num * 86400;
                            }
                        }
                    }
                }

                return AIParseResponse {
                    intent: AIIntentDto {
                        action: "create_link".to_string(),
                        amount_paisa: Some(paisa.to_string()),
                        recipient: None,
                        expires_in_seconds: Some(ttl),
                        note: None,
                    },
                    summary: format!(
                        "Create a payment link for {} paisa valid for {} seconds",
                        paisa, ttl
                    ),
                    confidence: 0.95,
                    flags,
                };
            }
        }
    }

    // 2. Match Direct Transfer (§4 Grammar)
    if let Some(caps) = TRANSFER_REGEX.captures(trimmed) {
        if let (Some(amt_m), Some(rec_m)) = (caps.get(1), caps.get(2)) {
            if let Some(paisa) = parse_natural_taka_to_paisa(amt_m.as_str()) {
                let recipient = rec_m.as_str().to_string();
                return AIParseResponse {
                    intent: AIIntentDto {
                        action: "transfer".to_string(),
                        amount_paisa: Some(paisa.to_string()),
                        recipient: Some(recipient.clone()),
                        expires_in_seconds: None,
                        note: None,
                    },
                    summary: format!("Transfer {} paisa to {}", paisa, recipient),
                    confidence: 0.95,
                    flags,
                };
            }
        }
    }

    // 3. Match Money Request (§4 Grammar)
    if let Some(caps) = REQUEST_REGEX.captures(trimmed) {
        if let (Some(amt_m), Some(rec_m)) = (caps.get(1), caps.get(2)) {
            if let Some(paisa) = parse_natural_taka_to_paisa(amt_m.as_str()) {
                let recipient = rec_m.as_str().to_string();
                return AIParseResponse {
                    intent: AIIntentDto {
                        action: "request".to_string(),
                        amount_paisa: Some(paisa.to_string()),
                        recipient: Some(recipient.clone()),
                        expires_in_seconds: None,
                        note: None,
                    },
                    summary: format!("Request {} paisa from {}", paisa, recipient),
                    confidence: 0.95,
                    flags,
                };
            }
        }
    }

    // 4. Match Balance
    if BALANCE_REGEX.is_match(trimmed) {
        return AIParseResponse {
            intent: AIIntentDto {
                action: "balance".to_string(),
                amount_paisa: None,
                recipient: None,
                expires_in_seconds: None,
                note: None,
            },
            summary: "Check current wallet balance".to_string(),
            confidence: 0.99,
            flags,
        };
    }

    // 5. Match History
    if HISTORY_REGEX.is_match(trimmed) {
        return AIParseResponse {
            intent: AIIntentDto {
                action: "history".to_string(),
                amount_paisa: None,
                recipient: None,
                expires_in_seconds: None,
                note: None,
            },
            summary: "View recent transaction history".to_string(),
            confidence: 0.99,
            flags,
        };
    }

    // 6. Fallback / Help (C30)
    AIParseResponse {
        intent: AIIntentDto {
            action: "help".to_string(),
            amount_paisa: None,
            recipient: None,
            expires_in_seconds: None,
            note: None,
        },
        summary: "I didn't recognize that command. Try: 'Send 500 to 01711000000' or 'Create link 200 for 2 hours'".to_string(),
        confidence: 0.1,
        flags,
    }
}
