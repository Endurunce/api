use sqlx::PgPool;
use uuid::Uuid;

use crate::models::profile::{Profile, ProfileInput, ProfilePatch};

pub async fn fetch_by_user(db: &PgPool, user_id: Uuid) -> Result<Option<Profile>, sqlx::Error> {
    sqlx::query_as::<_, Profile>(
        "SELECT id, user_id, name, date_of_birth, gender, running_experience, weekly_km, \
         time_5k, time_10k, time_half, time_marathon, rest_hr, max_hr, sleep_quality, complaints \
         FROM profiles WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Return a text summary of the user's profile for AI context.
pub async fn fetch_full_by_user(db: &PgPool, user_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    let profile = fetch_by_user(db, user_id).await?;
    Ok(profile.map(|p| {
        format!(
            "{}, {}, {} jaar, {} jaar ervaring, {:.0} km/week, rust-HR: {:?}, max-HR: {:?}",
            p.name,
            p.gender,
            p.age_years(),
            p.running_experience,
            p.weekly_km,
            p.rest_hr,
            p.max_hr,
        )
    }))
}

pub async fn upsert(db: &PgPool, user_id: Uuid, input: &ProfileInput) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query_as::<_, (Uuid,)>(
        r#"INSERT INTO profiles (user_id, name, date_of_birth, gender, running_experience, weekly_km,
            time_5k, time_10k, time_half, time_marathon, rest_hr, max_hr, sleep_quality, complaints)
           VALUES ($1, $2, $3, $4, COALESCE($5, 'two_to_five_years'), COALESCE($6, 0), $7, $8, $9, $10, $11, $12, $13, $14)
           ON CONFLICT (user_id) DO UPDATE SET
            name = EXCLUDED.name,
            date_of_birth = EXCLUDED.date_of_birth,
            gender = EXCLUDED.gender,
            running_experience = EXCLUDED.running_experience,
            weekly_km = EXCLUDED.weekly_km,
            time_5k = EXCLUDED.time_5k,
            time_10k = EXCLUDED.time_10k,
            time_half = EXCLUDED.time_half,
            time_marathon = EXCLUDED.time_marathon,
            rest_hr = EXCLUDED.rest_hr,
            max_hr = EXCLUDED.max_hr,
            sleep_quality = EXCLUDED.sleep_quality,
            complaints = EXCLUDED.complaints,
            updated_at = NOW()
           RETURNING id"#,
    )
    .bind(user_id)
    .bind(&input.name)
    .bind(input.date_of_birth)
    .bind(&input.gender)
    .bind(&input.running_experience)
    .bind(input.weekly_km)
    .bind(&input.time_5k)
    .bind(&input.time_10k)
    .bind(&input.time_half)
    .bind(&input.time_marathon)
    .bind(input.rest_hr)
    .bind(input.max_hr)
    .bind(&input.sleep_quality)
    .bind(&input.complaints)
    .fetch_one(db)
    .await?;

    Ok(row.0)
}

pub async fn patch(db: &PgPool, user_id: Uuid, patch: &ProfilePatch) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE profiles SET
            name = COALESCE($2, name),
            date_of_birth = COALESCE($3, date_of_birth),
            gender = COALESCE($4, gender),
            running_experience = COALESCE($5, running_experience),
            weekly_km = COALESCE($6, weekly_km),
            time_5k = COALESCE($7, time_5k),
            time_10k = COALESCE($8, time_10k),
            time_half = COALESCE($9, time_half),
            time_marathon = COALESCE($10, time_marathon),
            rest_hr = COALESCE($11, rest_hr),
            max_hr = COALESCE($12, max_hr),
            sleep_quality = COALESCE($13, sleep_quality),
            complaints = COALESCE($14, complaints),
            updated_at = NOW()
           WHERE user_id = $1"#,
    )
    .bind(user_id)
    .bind(&patch.name)
    .bind(patch.date_of_birth)
    .bind(&patch.gender)
    .bind(&patch.running_experience)
    .bind(patch.weekly_km)
    .bind(&patch.time_5k)
    .bind(&patch.time_10k)
    .bind(&patch.time_half)
    .bind(&patch.time_marathon)
    .bind(patch.rest_hr)
    .bind(patch.max_hr)
    .bind(&patch.sleep_quality)
    .bind(&patch.complaints)
    .execute(db)
    .await?;

    Ok(())
}
