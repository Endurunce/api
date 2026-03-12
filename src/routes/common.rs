use axum::{response::Redirect, Json};

/// Shared response type for OAuth callbacks that may return JSON or a redirect.
pub enum CallbackResponse {
    Json(Json<serde_json::Value>),
    Redirect(Redirect),
}

impl axum::response::IntoResponse for CallbackResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            CallbackResponse::Json(j) => j.into_response(),
            CallbackResponse::Redirect(r) => r.into_response(),
        }
    }
}
