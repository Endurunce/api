use sqlx::PgPool;
use uuid::Uuid;

use crate::models::activity::{Activity, ActivityInput};

pub async fn create(
    db: &PgPool,
    user_id: Uuid,
    input: &ActivityInput,
) -> Result<Uuid, sqlx::Error> {
    let source = input.source.as_deref().unwrap_or("manual");
    let activity_type = input.activity_type.as_deref().unwrap_or("run");

    let (id,) = sqlx::query_as::<_, (Uuid,)>(
        r#"INSERT INTO activities (user_id, session_id, source, source_id, activity_type,
            distance_km, duration_seconds, avg_pace_sec_km, avg_hr, max_hr, elevation_m, calories,
            feeling, pain, notes, started_at, completed_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, COALESCE($17, NOW()))
           RETURNING id"#,
    )
    .bind(user_id)
    .bind(input.session_id)
    .bind(source)
    .bind(&input.source_id)
    .bind(activity_type)
    .bind(input.distance_km)
    .bind(input.duration_seconds)
    .bind(input.avg_pace_sec_km)
    .bind(input.avg_hr)
    .bind(input.max_hr)
    .bind(input.elevation_m)
    .bind(input.calories)
    .bind(input.feeling)
    .bind(input.pain)
    .bind(&input.notes)
    .bind(input.started_at)
    .bind(input.completed_at)
    .fetch_one(db)
    .await?;

    Ok(id)
}

pub async fn fetch_by_user(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Activity>, sqlx::Error> {
    sqlx::query_as::<_, Activity>(
        r#"SELECT id, user_id, session_id, source, source_id, activity_type,
            distance_km, duration_seconds, avg_pace_sec_km, avg_hr, max_hr, elevation_m, calories,
            feeling, pain, notes, started_at, completed_at
           FROM activities WHERE user_id = $1
           ORDER BY completed_at DESC
           LIMIT $2 OFFSET $3"#,
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

pub async fn fetch_by_session(
    db: &PgPool,
    session_id: Uuid,
) -> Result<Option<Activity>, sqlx::Error> {
    sqlx::query_as::<_, Activity>(
        r#"SELECT id, user_id, session_id, source, source_id, activity_type,
            distance_km, duration_seconds, avg_pace_sec_km, avg_hr, max_hr, elevation_m, calories,
            feeling, pain, notes, started_at, completed_at
           FROM activities WHERE session_id = $1"#,
    )
    .bind(session_id)
    .fetch_optional(db)
    .await
}

pub async fn fetch_by_id(
    db: &PgPool,
    id: Uuid,
    user_id: Uuid,
) -> Result<Option<Activity>, sqlx::Error> {
    sqlx::query_as::<_, Activity>(
        r#"SELECT id, user_id, session_id, source, source_id, activity_type,
            distance_km, duration_seconds, avg_pace_sec_km, avg_hr, max_hr, elevation_m, calories,
            feeling, pain, notes, started_at, completed_at
           FROM activities WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}
