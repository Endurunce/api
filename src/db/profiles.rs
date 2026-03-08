use sqlx::PgPool;
use uuid::Uuid;
use serde_json::Value;

use crate::models::profile::Profile;

pub async fn upsert(db: &PgPool, profile: &Profile) -> Result<Uuid, sqlx::Error> {
    let race_goal_json = serde_json::to_value(&profile.race_goal).expect("serialization failed");
    let hr_zones_json  = profile.hr_zones.as_ref().map(|z| serde_json::to_value(z).unwrap());
    let max_dur_json: Value = serde_json::to_value(&profile.max_duration_per_day).unwrap();

    let training_days: Vec<i16> = profile.training_days.iter().map(|d| d.0 as i16).collect();
    let previous_injuries: Vec<String> = profile.previous_injuries.clone();

    let row = sqlx::query!(
        r#"
        INSERT INTO profiles (
            id, user_id, name, age, gender,
            running_years, weekly_km, previous_ultra,
            time_10k, time_half_marathon, time_marathon,
            race_goal, race_date, terrain,
            training_days, max_duration_per_day, long_run_day,
            max_hr, rest_hr, hr_zones,
            sleep_hours, complaints, previous_injuries
        ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8,
            $9, $10, $11,
            $12, $13, $14,
            $15, $16, $17,
            $18, $19, $20,
            $21, $22, $23
        )
        ON CONFLICT (user_id) DO UPDATE SET
            name = EXCLUDED.name,
            age = EXCLUDED.age,
            gender = EXCLUDED.gender,
            running_years = EXCLUDED.running_years,
            weekly_km = EXCLUDED.weekly_km,
            previous_ultra = EXCLUDED.previous_ultra,
            time_10k = EXCLUDED.time_10k,
            time_half_marathon = EXCLUDED.time_half_marathon,
            time_marathon = EXCLUDED.time_marathon,
            race_goal = EXCLUDED.race_goal,
            race_date = EXCLUDED.race_date,
            terrain = EXCLUDED.terrain,
            training_days = EXCLUDED.training_days,
            max_duration_per_day = EXCLUDED.max_duration_per_day,
            long_run_day = EXCLUDED.long_run_day,
            max_hr = EXCLUDED.max_hr,
            rest_hr = EXCLUDED.rest_hr,
            hr_zones = EXCLUDED.hr_zones,
            sleep_hours = EXCLUDED.sleep_hours,
            complaints = EXCLUDED.complaints,
            previous_injuries = EXCLUDED.previous_injuries,
            updated_at = NOW()
        RETURNING id
        "#,
        profile.id,
        profile.user_id,
        profile.name,
        profile.age as i16,
        format!("{:?}", profile.gender).to_lowercase(),
        format!("{:?}", profile.running_years).to_lowercase(),
        profile.weekly_km,
        format!("{:?}", profile.previous_ultra).to_lowercase(),
        profile.time_10k,
        profile.time_half_marathon,
        profile.time_marathon,
        race_goal_json,
        profile.race_date,
        format!("{:?}", profile.terrain).to_lowercase(),
        &training_days,
        max_dur_json,
        profile.long_run_day.0 as i16,
        profile.max_hr.map(|h| h as i16),
        profile.rest_hr as i16,
        hr_zones_json,
        format!("{:?}", profile.sleep_hours).to_lowercase(),
        profile.complaints,
        &previous_injuries,
    )
    .fetch_one(db)
    .await?;

    Ok(row.id)
}

pub async fn fetch_by_user(db: &PgPool, user_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT id FROM profiles WHERE user_id = $1",
        user_id,
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| r.id))
}
