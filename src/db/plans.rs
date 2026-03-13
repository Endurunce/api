use sqlx::PgPool;
use uuid::Uuid;

use crate::models::plan::{
    FullPlan, FullWeek, Plan, PlanInsert, PlanWeek, Session,
};

/// Deactivate all existing plans for a user.
pub async fn deactivate_all(db: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE plans SET active = false, updated_at = NOW() WHERE user_id = $1 AND active = true")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Insert a full plan with weeks and sessions in a single transaction.
pub async fn insert_full(db: &PgPool, input: &PlanInsert) -> Result<Uuid, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Deactivate existing plans
    sqlx::query("UPDATE plans SET active = false, updated_at = NOW() WHERE user_id = $1 AND active = true")
        .bind(input.user_id)
        .execute(&mut *tx)
        .await?;

    // Insert plan
    let (plan_id,) = sqlx::query_as::<_, (Uuid,)>(
        r#"INSERT INTO plans (user_id, race_goal, race_goal_km, race_time_goal, race_date, terrain, num_weeks, start_km)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id"#,
    )
    .bind(input.user_id)
    .bind(&input.race_goal)
    .bind(input.race_goal_km)
    .bind(&input.race_time_goal)
    .bind(input.race_date)
    .bind(&input.terrain)
    .bind(input.weeks.len() as i16)
    .bind(input.start_km)
    .fetch_one(&mut *tx)
    .await?;

    // Insert weeks and sessions
    for week in &input.weeks {
        let (week_id,) = sqlx::query_as::<_, (Uuid,)>(
            r#"INSERT INTO plan_weeks (plan_id, week_number, phase, target_km, is_recovery, notes)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id"#,
        )
        .bind(plan_id)
        .bind(week.week_number)
        .bind(&week.phase)
        .bind(week.target_km)
        .bind(week.is_recovery)
        .bind(&week.notes)
        .fetch_one(&mut *tx)
        .await?;

        for session in &week.sessions {
            sqlx::query(
                r#"INSERT INTO sessions (plan_week_id, user_id, weekday, session_type, target_km, target_duration_min, target_hr_zones, notes, sort_order)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            )
            .bind(week_id)
            .bind(input.user_id)
            .bind(session.weekday)
            .bind(&session.session_type)
            .bind(session.target_km)
            .bind(session.target_duration_min)
            .bind(&session.target_hr_zones)
            .bind(&session.notes)
            .bind(session.sort_order)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(plan_id)
}

/// Fetch the active plan for a user, fully assembled with weeks and sessions.
pub async fn fetch_active(db: &PgPool, user_id: Uuid) -> Result<Option<FullPlan>, sqlx::Error> {
    let plan = sqlx::query_as::<_, Plan>(
        "SELECT id, user_id, race_goal, race_goal_km, race_time_goal, race_date, terrain, num_weeks, start_km, active \
         FROM plans WHERE user_id = $1 AND active = true ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    match plan {
        Some(plan) => assemble_full_plan(db, plan).await.map(Some),
        None => Ok(None),
    }
}

/// Fetch a plan by ID (scoped to user).
pub async fn fetch_by_id(
    db: &PgPool,
    plan_id: Uuid,
    user_id: Uuid,
) -> Result<Option<FullPlan>, sqlx::Error> {
    let plan = sqlx::query_as::<_, Plan>(
        "SELECT id, user_id, race_goal, race_goal_km, race_time_goal, race_date, terrain, num_weeks, start_km, active \
         FROM plans WHERE id = $1 AND user_id = $2",
    )
    .bind(plan_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    match plan {
        Some(plan) => assemble_full_plan(db, plan).await.map(Some),
        None => Ok(None),
    }
}

/// Assemble a full plan from a Plan row by fetching weeks and sessions.
async fn assemble_full_plan(db: &PgPool, plan: Plan) -> Result<FullPlan, sqlx::Error> {
    let weeks = sqlx::query_as::<_, PlanWeek>(
        "SELECT id, plan_id, week_number, phase, target_km, is_recovery, notes \
         FROM plan_weeks WHERE plan_id = $1 ORDER BY week_number",
    )
    .bind(plan.id)
    .fetch_all(db)
    .await?;

    let sessions = sqlx::query_as::<_, Session>(
        "SELECT s.id, s.plan_week_id, s.user_id, s.weekday, s.session_type, s.target_km, \
         s.target_duration_min, s.target_hr_zones, s.notes, s.sort_order \
         FROM sessions s \
         JOIN plan_weeks pw ON s.plan_week_id = pw.id \
         WHERE pw.plan_id = $1 \
         ORDER BY pw.week_number, s.weekday",
    )
    .bind(plan.id)
    .fetch_all(db)
    .await?;

    let full_weeks: Vec<FullWeek> = weeks
        .into_iter()
        .map(|week| {
            let week_sessions: Vec<Session> = sessions
                .iter()
                .filter(|s| s.plan_week_id == week.id)
                .cloned()
                .collect();
            FullWeek {
                week,
                sessions: week_sessions,
            }
        })
        .collect();

    Ok(FullPlan {
        plan,
        weeks: full_weeks,
    })
}

/// Update a single session's fields.
pub async fn update_session(
    db: &PgPool,
    session_id: Uuid,
    user_id: Uuid,
    session_type: Option<&str>,
    target_km: Option<f32>,
    notes: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"UPDATE sessions SET
            session_type = COALESCE($3, session_type),
            target_km = COALESCE($4, target_km),
            notes = COALESCE($5, notes)
           WHERE id = $1 AND user_id = $2"#,
    )
    .bind(session_id)
    .bind(user_id)
    .bind(session_type)
    .bind(target_km)
    .bind(notes)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Fetch a single session by ID.
pub async fn fetch_session(
    db: &PgPool,
    session_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Session>, sqlx::Error> {
    sqlx::query_as::<_, Session>(
        "SELECT id, plan_week_id, user_id, weekday, session_type, target_km, target_duration_min, \
         target_hr_zones, notes, sort_order FROM sessions WHERE id = $1 AND user_id = $2",
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Fetch all sessions for a specific week in a plan.
pub async fn fetch_week_sessions(
    db: &PgPool,
    plan_id: Uuid,
    week_number: i16,
    user_id: Uuid,
) -> Result<Vec<Session>, sqlx::Error> {
    sqlx::query_as::<_, Session>(
        r#"SELECT s.id, s.plan_week_id, s.user_id, s.weekday, s.session_type, s.target_km,
            s.target_duration_min, s.target_hr_zones, s.notes, s.sort_order
           FROM sessions s
           JOIN plan_weeks pw ON s.plan_week_id = pw.id
           WHERE pw.plan_id = $1 AND pw.week_number = $2 AND s.user_id = $3
           ORDER BY s.weekday"#,
    )
    .bind(plan_id)
    .bind(week_number)
    .bind(user_id)
    .fetch_all(db)
    .await
}
