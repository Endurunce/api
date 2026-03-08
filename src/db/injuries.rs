use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::injury::InjuryReport;

pub async fn insert(
    db: &PgPool,
    injury: &InjuryReport,
    plan_id: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    let locations: Vec<String> = injury.locations.iter()
        .map(|l| format!("{:?}", l).to_lowercase())
        .collect();

    let row = sqlx::query!(
        r#"
        INSERT INTO injury_reports (
            id, user_id, plan_id,
            reported_at, locations, severity,
            can_walk, can_run, description, recovery_status
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id
        "#,
        injury.id,
        injury.user_id,
        plan_id,
        injury.reported_at,
        &locations,
        injury.severity as i16,
        injury.can_walk,
        injury.can_run,
        injury.description,
        format!("{:?}", injury.recovery_status).to_lowercase(),
    )
    .fetch_one(db)
    .await?;

    Ok(row.id)
}

pub async fn fetch_active(db: &PgPool, user_id: Uuid) -> Result<Vec<InjuryRow>, sqlx::Error> {
    sqlx::query_as!(
        InjuryRow,
        r#"
        SELECT id, severity, can_run, recovery_status, reported_at, description
        FROM injury_reports
        WHERE user_id = $1 AND recovery_status != 'resolved'
        ORDER BY reported_at DESC
        "#,
        user_id,
    )
    .fetch_all(db)
    .await
}

/// Resolve an injury — scoped to user_id for ownership safety.
/// Returns `true` if a row was updated, `false` if not found or already resolved.
pub async fn resolve_by_user(
    db: &PgPool,
    injury_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        UPDATE injury_reports
        SET recovery_status = 'resolved', resolved_at = CURRENT_DATE, updated_at = NOW()
        WHERE id = $1 AND user_id = $2 AND recovery_status != 'resolved'
        "#,
        injury_id,
        user_id,
    )
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub struct InjuryRow {
    pub id: Uuid,
    pub severity: i16,
    pub can_run: bool,
    pub recovery_status: String,
    pub reported_at: NaiveDate,
    pub description: Option<String>,
}
