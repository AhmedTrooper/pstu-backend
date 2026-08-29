use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AIParseRequest {
    #[validate(length(
        min = 1,
        max = 500,
        message = "text must be between 1 and 500 characters"
    ))]
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct AIIntentDto {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_paisa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AIParseResponse {
    pub intent: AIIntentDto,
    pub summary: String,
    pub confidence: f32,
    pub flags: Vec<String>,
}
