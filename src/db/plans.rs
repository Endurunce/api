use sqlx::PgPool;
use uuid::Uuid;
use chrono::NaiveDate;

use crate::models::plan::Plan;

pub async fn insert(
    db: &PgPool,
    plan: &Plan,
    profile_id: Uuid,
    race_date: Option<NaiveDate>,
    race_goal: &str,
) -> Result<Uuid, sqlx::Error> {
    let weeks_json = serde_json::to_value(&plan.weeks).expect("plan serialization failed");
    let num_weeks = plan.weeks.len() as i16;

    let row = sqlx::query!(
        r#"
        INSERT INTO plans (id, user_id, profile_id, num_weeks, race_date, race_goal, weeks)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
        plan.id,
        plan.user_id,
        profile_id,
        num_weeks,
        race_date,
        race_goal,
        weeks_json,
    )
    .fetch_one(db)
    .await?;

    Ok(row.id)
}

pub async fn fetch_active(db: &PgPool, user_id: Uuid) -> Result<Option<Plan>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, weeks
        FROM plans
        WHERE user_id = $1 AND active = TRUE
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        user_id,
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else { return Ok(None) };

    let weeks = serde_json::from_value(row.weeks)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

    Ok(Some(Plan { id: row.id, user_id: row.user_id, weeks }))
}

pub async fn fetch_by_id(db: &PgPool, plan_id: Uuid, user_id: Uuid) -> Result<Option<Plan>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, weeks
        FROM plans
        WHERE id = $1 AND user_id = $2
        "#,
        plan_id,
        user_id,
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else { return Ok(None) };

    let weeks = serde_json::from_value(row.weeks)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

    Ok(Some(Plan { id: row.id, user_id: row.user_id, weeks }))
}

/// Persist updated weeks back to the database (after injury adaptation, feedback, etc.)
pub async fn update_weeks(db: &PgPool, plan_id: Uuid, weeks: &[crate::models::plan::Week]) -> Result<(), sqlx::Error> {
    let weeks_json = serde_json::to_value(weeks).expect("serialization failed");

    sqlx::query!(
        r#"
        UPDATE plans
        SET weeks = $1, updated_at = NOW()
        WHERE id = $2
        "#,
        weeks_json,
        plan_id,
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Fetch the active plan with its creation timestamp (used to calculate current week).
pub struct ActivePlanWithMeta {
    pub plan: Plan,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn fetch_active_with_meta(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Option<ActivePlanWithMeta>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, weeks, created_at
        FROM plans
        WHERE user_id = $1 AND active = TRUE
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        user_id,
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else { return Ok(None) };

    let weeks = serde_json::from_value(row.weeks)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

    Ok(Some(ActivePlanWithMeta {
        plan: Plan { id: row.id, user_id: row.user_id, weeks },
        created_at: row.created_at,
    }))
}

pub async fn deactivate_all(db: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE plans SET active = FALSE WHERE user_id = $1",
        user_id,
    )
    .execute(db)
    .await?;
    Ok(())
}
