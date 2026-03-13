use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Activity matching the `activities` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Activity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub session_id: Option<Uuid>,
    pub source: String,
    pub source_id: Option<String>,
    pub activity_type: String,
    pub distance_km: Option<f32>,
    pub duration_seconds: Option<i32>,
    pub avg_pace_sec_km: Option<i32>,
    pub avg_hr: Option<i16>,
    pub max_hr: Option<i16>,
    pub elevation_m: Option<f32>,
    pub calories: Option<i32>,
    pub feeling: Option<i16>,
    pub pain: Option<bool>,
    pub notes: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: DateTime<Utc>,
}

/// Input for creating an activity.
#[derive(Debug, Deserialize)]
pub struct ActivityInput {
    pub session_id: Option<Uuid>,
    pub source: Option<String>,
    pub source_id: Option<String>,
    pub activity_type: Option<String>,
    pub distance_km: Option<f32>,
    pub duration_seconds: Option<i32>,
    pub avg_pace_sec_km: Option<i32>,
    pub avg_hr: Option<i16>,
    pub max_hr: Option<i16>,
    pub elevation_m: Option<f32>,
    pub calories: Option<i32>,
    pub feeling: Option<i16>,
    pub pain: Option<bool>,
    pub notes: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}
