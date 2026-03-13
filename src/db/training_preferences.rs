use sqlx::PgPool;
use uuid::Uuid;

use crate::models::training_preferences::{TrainingPreferences, TrainingPreferencesInput};

pub async fn fetch_by_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Option<TrainingPreferences>, sqlx::Error> {
    sqlx::query_as::<_, TrainingPreferences>(
        "SELECT id, user_id, training_days, long_run_day, strength_days, max_duration_per_day, terrain \
         FROM training_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub async fn upsert(
    db: &PgPool,
    user_id: Uuid,
    input: &TrainingPreferencesInput,
) -> Result<Uuid, sqlx::Error> {
    let long_run_day = input.long_run_day.unwrap_or(6);
    let strength_days = input.strength_days.clone().unwrap_or_default();
    let max_dur = input
        .max_duration_per_day
        .clone()
        .unwrap_or(serde_json::json!([]));
    let terrain = input.terrain.clone().unwrap_or_else(|| "road".into());

    let row = sqlx::query_as::<_, (Uuid,)>(
        r#"INSERT INTO training_preferences (user_id, training_days, long_run_day, strength_days, max_duration_per_day, terrain)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (user_id) DO UPDATE SET
            training_days = EXCLUDED.training_days,
            long_run_day = EXCLUDED.long_run_day,
            strength_days = EXCLUDED.strength_days,
            max_duration_per_day = EXCLUDED.max_duration_per_day,
            terrain = EXCLUDED.terrain,
            updated_at = NOW()
           RETURNING id"#,
    )
    .bind(user_id)
    .bind(&input.training_days)
    .bind(long_run_day)
    .bind(&strength_days)
    .bind(&max_dur)
    .bind(&terrain)
    .fetch_one(db)
    .await?;

    Ok(row.0)
}
