use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Training preferences matching the `training_preferences` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrainingPreferences {
    pub id: Uuid,
    pub user_id: Uuid,
    pub training_days: Vec<i16>,
    pub long_run_day: i16,
    pub strength_days: Vec<i16>,
    pub max_duration_per_day: serde_json::Value,
    pub terrain: String,
}

/// Payload for creating/updating training preferences.
#[derive(Debug, Deserialize)]
pub struct TrainingPreferencesInput {
    pub training_days: Vec<i16>,
    pub long_run_day: Option<i16>,
    pub strength_days: Option<Vec<i16>>,
    pub max_duration_per_day: Option<serde_json::Value>,
    pub terrain: Option<String>,
}
