use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::AdminClaims,
    db,
    errors::{AppError, ApiResult},
    AppState,
};

/// GET /api/admin/stats — aggregated platform statistics.
///
/// **Auth:** Bearer JWT with `is_admin=true` required. Returns 403 for non-admins.
///
/// **Response:** 200 with stats JSON (user count, plan count, etc.).
pub async fn stats(
    State(state): State<AppState>,
    _admin: AdminClaims,
) -> ApiResult<Json<serde_json::Value>> {
    let stats = db::users::fetch_stats(&state.db).await?;
    Ok(Json(stats))
}

#[derive(Debug, Deserialize)]
pub struct UsersParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub q: Option<String>,
}

/// GET /api/admin/users — paginated user list with optional search.
///
/// **Auth:** Bearer JWT with `is_admin=true` required.
///
/// **Query parameters:** `page?`, `per_page?` (max 100), `q?` (search query).
///
/// **Response:** 200 with `{ users, total, page, per_page, total_pages }`.
pub async fn list_users(
    State(state): State<AppState>,
    _admin: AdminClaims,
    Query(params): Query<UsersParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).min(100);

    let (users, total) = db::users::fetch_all_admin(
        &state.db,
        page,
        per_page,
        params.q.as_deref(),
    ).await?;

    Ok(Json(serde_json::json!({
        "users": users,
        "total": total,
        "page": page,
        "per_page": per_page,
        "total_pages": (total as f64 / per_page as f64).ceil() as i64,
    })))
}

#[derive(Debug, Deserialize)]
pub struct SetAdminRequest {
    pub is_admin: bool,
}

/// PATCH /api/admin/users/:id/admin — grant or revoke admin status.
///
/// **Auth:** Bearer JWT with `is_admin=true` required. Self-demotion is prevented.
///
/// **Request body:** `{ is_admin: bool }`.
///
/// **Response:** 200 with `{ user_id, is_admin }`.
pub async fn set_admin(
    State(state): State<AppState>,
    admin: AdminClaims,
    Path(user_id): Path<Uuid>,
    Json(req): Json<SetAdminRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Prevent self-demotion
    if admin.0.sub == user_id && !req.is_admin {
        return Err(AppError::BadRequest("Cannot remove your own admin status".into()));
    }

    db::users::set_admin(&state.db, user_id, req.is_admin).await?;

    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "is_admin": req.is_admin,
    })))
}
