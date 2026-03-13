use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Slimmed-down profile matching the `profiles` table.
/// Training preferences live in TrainingPreferences now.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Profile {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub date_of_birth: NaiveDate,
    pub gender: String,
    pub running_experience: String,
    pub weekly_km: f32,
    pub time_5k: Option<String>,
    pub time_10k: Option<String>,
    pub time_half: Option<String>,
    pub time_marathon: Option<String>,
    pub rest_hr: Option<i16>,
    pub max_hr: Option<i16>,
    pub sleep_quality: Option<String>,
    pub complaints: Option<String>,
}

/// Payload for creating/updating a profile.
#[derive(Debug, Deserialize)]
pub struct ProfileInput {
    pub name: String,
    pub date_of_birth: NaiveDate,
    pub gender: String,
    pub running_experience: Option<String>,
    pub weekly_km: Option<f32>,
    pub time_5k: Option<String>,
    pub time_10k: Option<String>,
    pub time_half: Option<String>,
    pub time_marathon: Option<String>,
    pub rest_hr: Option<i16>,
    pub max_hr: Option<i16>,
    pub sleep_quality: Option<String>,
    pub complaints: Option<String>,
}

/// Partial update payload.
#[derive(Debug, Deserialize)]
pub struct ProfilePatch {
    pub name: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    pub running_experience: Option<String>,
    pub weekly_km: Option<f32>,
    pub time_5k: Option<String>,
    pub time_10k: Option<String>,
    pub time_half: Option<String>,
    pub time_marathon: Option<String>,
    pub rest_hr: Option<i16>,
    pub max_hr: Option<i16>,
    pub sleep_quality: Option<String>,
    pub complaints: Option<String>,
}

impl Profile {
    /// Calculate the user's age in years.
    pub fn age_years(&self) -> u32 {
        let today = chrono::Local::now().date_naive();
        let mut age = (today.year() - self.date_of_birth.year()) as u32;
        if today.ordinal() < self.date_of_birth.ordinal() {
            age = age.saturating_sub(1);
        }
        age
    }
}

use chrono::Datelike;
