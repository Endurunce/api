use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::injury::{Injury, InjuryInput};

pub async fn insert(
    db: &PgPool,
    user_id: Uuid,
    input: &InjuryInput,
) -> Result<Uuid, sqlx::Error> {
    let (id,) = sqlx::query_as::<_, (Uuid,)>(
        r#"INSERT INTO injuries (user_id, locations, severity, can_walk, can_run, description)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id"#,
    )
    .bind(user_id)
    .bind(&input.locations)
    .bind(input.severity)
    .bind(input.can_walk)
    .bind(input.can_run)
    .bind(&input.description)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// List active (non-resolved) injuries for a user.
pub async fn list_active(db: &PgPool, user_id: Uuid) -> Result<Vec<Injury>, sqlx::Error> {
    sqlx::query_as::<_, Injury>(
        "SELECT id, user_id, locations, severity, can_walk, can_run, description, status, reported_at, resolved_at \
         FROM injuries WHERE user_id = $1 AND status != 'resolved' ORDER BY reported_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// List all injuries for a user (history).
pub async fn list_all(db: &PgPool, user_id: Uuid) -> Result<Vec<Injury>, sqlx::Error> {
    sqlx::query_as::<_, Injury>(
        "SELECT id, user_id, locations, severity, can_walk, can_run, description, status, reported_at, resolved_at \
         FROM injuries WHERE user_id = $1 ORDER BY reported_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// Resolve an injury.
pub async fn resolve(
    db: &PgPool,
    injury_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE injuries SET status = 'resolved', resolved_at = $3, updated_at = NOW() \
         WHERE id = $1 AND user_id = $2 AND status != 'resolved'",
    )
    .bind(injury_id)
    .bind(user_id)
    .bind(chrono::Local::now().date_naive())
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Fetch a single injury by ID.
pub async fn fetch_by_id(
    db: &PgPool,
    injury_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Injury>, sqlx::Error> {
    sqlx::query_as::<_, Injury>(
        "SELECT id, user_id, locations, severity, can_walk, can_run, description, status, reported_at, resolved_at \
         FROM injuries WHERE id = $1 AND user_id = $2",
    )
    .bind(injury_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}
