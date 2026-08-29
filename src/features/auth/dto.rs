use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    #[validate(length(
        min = 1,
        max = 100,
        message = "name must be between 1 and 100 characters"
    ))]
    pub name: String,

    #[validate(length(
        min = 8,
        max = 20,
        message = "phone must be a valid number between 8 and 20 characters"
    ))]
    pub phone: String,

    #[validate(length(min = 8, max = 128, message = "password must be at least 8 characters"))]
    pub password: String,

    #[validate(regex(path = *crate::core::money::PIN_REGEX, message = "PIN must be 4 to 6 digits"))]
    pub pin: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub id: Uuid,
    pub account_number: String,
    pub name: String,
    pub phone: String,
    pub balance: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    #[validate(length(min = 8, max = 20, message = "phone must be a valid number"))]
    pub phone: String,

    #[validate(length(min = 1, message = "password cannot be empty"))]
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserDto {
    pub id: Uuid,
    pub account_number: String,
    pub name: String,
    pub phone: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: UserDto,
    pub balance: String,
}

#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PinChangeReq {
    pub current_pin: String,
    #[validate(regex(path = *crate::core::money::PIN_REGEX, message = "new PIN must be 4 to 6 digits"))]
    pub new_pin: String,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PinResetReq {
    pub password: String,
    #[validate(regex(path = *crate::core::money::PIN_REGEX, message = "new PIN must be 4 to 6 digits"))]
    pub new_pin: String,
}

#[derive(Debug, Serialize)]
pub struct PinUpdatedRes {
    pub ok: bool,
    pub pin_updated_at: DateTime<Utc>,
}
