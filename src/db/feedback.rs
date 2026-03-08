use sqlx::PgPool;
use uuid::Uuid;

pub async fn upsert(
    db: &PgPool,
    user_id: Uuid,
    plan_id: Uuid,
    week_number: i16,
    weekday: i16,
    feeling: i16,
    pain: bool,
    notes: Option<&str>,
    actual_km: Option<f32>,
    ai_advice_json: Option<serde_json::Value>,
) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        INSERT INTO session_feedback (
            user_id, plan_id, week_number, weekday,
            feeling, pain, notes, actual_km, ai_advice
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (plan_id, week_number, weekday) DO UPDATE SET
            feeling      = EXCLUDED.feeling,
            pain         = EXCLUDED.pain,
            notes        = EXCLUDED.notes,
            actual_km    = EXCLUDED.actual_km,
            ai_advice    = EXCLUDED.ai_advice,
            completed_at = NOW()
        RETURNING id
        "#,
        user_id,
        plan_id,
        week_number,
        weekday,
        feeling,
        pain,
        notes,
        actual_km,
        ai_advice_json,
    )
    .fetch_one(db)
    .await?;

    Ok(row.id)
}
