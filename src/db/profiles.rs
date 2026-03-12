use sqlx::PgPool;
use uuid::Uuid;
use chrono::NaiveDate;
use serde_json::Value;

use crate::models::profile::Profile;

pub async fn upsert(db: &PgPool, profile: &Profile) -> Result<Uuid, sqlx::Error> {
    let race_goal_json = serde_json::to_value(&profile.race_goal).expect("serialization failed");
    let hr_zones_json  = profile.hr_zones.as_ref().map(|z| serde_json::to_value(z).unwrap());
    let max_dur_json: Value = serde_json::to_value(&profile.max_duration_per_day).unwrap();

    let training_days: Vec<i16>  = profile.training_days.iter().map(|d| d.0 as i16).collect();
    let strength_days: Vec<i16>  = profile.strength_days.iter().map(|d| d.0 as i16).collect();
    let previous_injuries: Vec<String> = profile.previous_injuries.clone();

    let row = sqlx::query!(
        r#"
        INSERT INTO profiles (
            id, user_id, name, date_of_birth, gender,
            running_years, weekly_km, previous_ultra,
            time_10k, time_half_marathon, time_marathon,
            race_goal, race_time_goal, race_date, terrain,
            training_days, strength_days, max_duration_per_day, long_run_day,
            max_hr, rest_hr, hr_zones,
            sleep_hours, complaints, previous_injuries
        ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8,
            $9, $10, $11,
            $12, $13, $14, $15,
            $16, $17, $18, $19,
            $20, $21, $22,
            $23, $24, $25
        )
        ON CONFLICT (user_id) DO UPDATE SET
            name = EXCLUDED.name,
            date_of_birth = EXCLUDED.date_of_birth,
            gender = EXCLUDED.gender,
            running_years = EXCLUDED.running_years,
            weekly_km = EXCLUDED.weekly_km,
            previous_ultra = EXCLUDED.previous_ultra,
            time_10k = EXCLUDED.time_10k,
            time_half_marathon = EXCLUDED.time_half_marathon,
            time_marathon = EXCLUDED.time_marathon,
            race_goal = EXCLUDED.race_goal,
            race_time_goal = EXCLUDED.race_time_goal,
            race_date = EXCLUDED.race_date,
            terrain = EXCLUDED.terrain,
            training_days = EXCLUDED.training_days,
            strength_days = EXCLUDED.strength_days,
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
        profile.date_of_birth,
        format!("{:?}", profile.gender).to_lowercase(),
        format!("{:?}", profile.running_years).to_lowercase(),
        profile.weekly_km,
        format!("{:?}", profile.previous_ultra).to_lowercase(),
        profile.time_10k,
        profile.time_half_marathon,
        profile.time_marathon,
        race_goal_json,
        profile.race_time_goal,
        profile.race_date,
        format!("{:?}", profile.terrain).to_lowercase(),
        &training_days,
        &strength_days,
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

/// Fetch user profile for the /api/profiles/me route
pub async fn fetch_me(db: &PgPool, user_id: Uuid) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT name, date_of_birth, gender, race_goal, race_date, terrain,
               weekly_km, running_years
        FROM profiles
        WHERE user_id = $1
        "#,
        user_id,
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| serde_json::json!({
        "name":          r.name,
        "date_of_birth": r.date_of_birth,
        "gender":        r.gender,
        "race_goal":     r.race_goal,
        "race_date":     r.race_date,
        "terrain":       r.terrain,
        "weekly_km":     r.weekly_km,
        "running_years": r.running_years,
    })))
}

/// Update editable personal fields on the profile
pub async fn update_me(
    db: &PgPool,
    user_id: Uuid,
    name: Option<&str>,
    date_of_birth: Option<NaiveDate>,
    gender: Option<&str>,
    weekly_km: Option<f32>,
    running_years: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE profiles SET
            name          = COALESCE($1, name),
            date_of_birth = COALESCE($2, date_of_birth),
            gender        = COALESCE($3, gender),
            weekly_km     = COALESCE($4::float4, weekly_km),
            running_years = COALESCE($5, running_years),
            updated_at    = NOW()
        WHERE user_id = $6
        "#,
        name,
        date_of_birth,
        gender,
        weekly_km,
        running_years,
        user_id,
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Returns the profile as a compact JSON string for AI context injection.
pub async fn fetch_full_by_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT
            name, date_of_birth, gender,
            running_years, weekly_km,
            race_goal, race_date, terrain,
            training_days,
            max_hr, rest_hr,
            sleep_hours, complaints, previous_injuries
        FROM profiles
        WHERE user_id = $1
        "#,
        user_id,
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| {
        use chrono::{Local, Datelike};
        let today = Local::now().date_naive();
        let dob = r.date_of_birth;
        let mut age = today.year() - dob.year();
        if today.month() < dob.month() || (today.month() == dob.month() && today.day() < dob.day()) {
            age -= 1;
        }
        format!(
            "Naam: {}, Leeftijd: {}, Geslacht: {}, Ervaring: {}, Weekkm: {:.0}, Doel: {}, Terrein: {}, Slaap: {}, Klachten: {}",
            r.name,
            age,
            r.gender,
            r.running_years,
            r.weekly_km,
            serde_json::to_string(&r.race_goal).unwrap_or_default(),
            r.terrain,
            r.sleep_hours,
            r.complaints.as_deref().unwrap_or("geen"),
        )
    }))
}
