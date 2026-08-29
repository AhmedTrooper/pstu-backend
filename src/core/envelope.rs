use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Meta {
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiResponse<T> {
    pub data: T,
    pub meta: Meta,
}

impl<T> ApiResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            meta: Meta {
                request_id: uuid::Uuid::new_v4().to_string(),
            },
        }
    }

    pub fn with_request_id(data: T, request_id: impl Into<String>) -> Self {
        Self {
            data,
            meta: Meta {
                request_id: request_id.into(),
            },
        }
    }
}
