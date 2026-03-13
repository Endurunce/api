use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Plan metadata matching the `plans` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Plan {
    pub id: Uuid,
    pub user_id: Uuid,
    pub race_goal: String,
    pub race_goal_km: Option<f32>,
    pub race_time_goal: Option<String>,
    pub race_date: Option<NaiveDate>,
    pub terrain: String,
    pub num_weeks: i16,
    pub start_km: f32,
    pub active: bool,
}

/// A week in a plan, matching `plan_weeks` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PlanWeek {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub week_number: i16,
    pub phase: String,
    pub target_km: f32,
    pub is_recovery: bool,
    pub notes: Option<String>,
}

/// A training session, matching `sessions` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub plan_week_id: Uuid,
    pub user_id: Uuid,
    pub weekday: i16,
    pub session_type: String,
    pub target_km: f32,
    pub target_duration_min: Option<i16>,
    pub target_hr_zones: Option<Vec<i16>>,
    pub notes: Option<String>,
    pub sort_order: i16,
}

/// Full plan with weeks and sessions (assembled from JOINs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullPlan {
    #[serde(flatten)]
    pub plan: Plan,
    pub weeks: Vec<FullWeek>,
}

/// A week with its sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullWeek {
    #[serde(flatten)]
    pub week: PlanWeek,
    pub sessions: Vec<Session>,
}

/// Input for generating a plan.
#[derive(Debug, Deserialize)]
pub struct GeneratePlanInput {
    pub profile: serde_json::Value,
}

/// Input for creating a plan (from schedule generator or AI).
#[derive(Debug, Clone)]
pub struct PlanInsert {
    pub user_id: Uuid,
    pub race_goal: String,
    pub race_goal_km: Option<f32>,
    pub race_time_goal: Option<String>,
    pub race_date: Option<NaiveDate>,
    pub terrain: String,
    pub start_km: f32,
    pub weeks: Vec<WeekInsert>,
}

/// Input for inserting a week.
#[derive(Debug, Clone)]
pub struct WeekInsert {
    pub week_number: i16,
    pub phase: String,
    pub target_km: f32,
    pub is_recovery: bool,
    pub notes: Option<String>,
    pub sessions: Vec<SessionInsert>,
}

/// Input for inserting a session.
#[derive(Debug, Clone)]
pub struct SessionInsert {
    pub weekday: i16,
    pub session_type: String,
    pub target_km: f32,
    pub target_duration_min: Option<i16>,
    pub target_hr_zones: Option<Vec<i16>>,
    pub notes: Option<String>,
    pub sort_order: i16,
}

// ── Helper methods ─────────────────────────────────────────────────────────────

/// Check if a session type is a running type.
pub fn is_running_type(session_type: &str) -> bool {
    matches!(
        session_type,
        "easy" | "tempo" | "long" | "interval" | "race" | "hike"
    )
}

/// Get the distance for a race goal string.
pub fn race_goal_distance_km(goal: &str) -> f32 {
    match goal {
        "5k" => 5.0,
        "10k" => 10.0,
        "half_marathon" => 21.1,
        "marathon" => 42.2,
        "sub3_marathon" => 42.2,
        "50k" => 50.0,
        "100k" => 100.0,
        _ => 42.2,
    }
}

/// Get peak weekly km for a race goal.
pub fn race_goal_peak_km(goal: &str) -> f32 {
    match goal {
        "5k" => 35.0,
        "10k" => 45.0,
        "half_marathon" => 55.0,
        "marathon" | "sub3_marathon" => 70.0,
        "50k" => 80.0,
        "100k" => 95.0,
        _ => 70.0,
    }
}

/// Get min/max weeks for a race goal.
pub fn race_goal_min_weeks(goal: &str) -> u8 {
    match goal {
        "5k" => 6,
        "10k" => 8,
        "half_marathon" => 10,
        "marathon" | "sub3_marathon" => 12,
        "50k" | "100k" => 16,
        _ => 12,
    }
}

pub fn race_goal_max_weeks(goal: &str) -> u8 {
    match goal {
        "5k" => 12,
        "10k" => 16,
        "half_marathon" => 20,
        "marathon" | "sub3_marathon" => 24,
        "50k" | "100k" => 30,
        _ => 24,
    }
}

/// Check if goal is a speed goal.
pub fn is_speed_goal(goal: &str) -> bool {
    matches!(goal, "sub3_marathon")
}

/// Check if goal is ultra.
pub fn is_ultra(goal: &str) -> bool {
    matches!(goal, "50k" | "100k")
}

/// Check if goal is marathon or longer.
pub fn is_marathon_or_longer(goal: &str) -> bool {
    matches!(goal, "marathon" | "sub3_marathon" | "50k" | "100k")
}

/// Get phase label for display.
pub fn phase_label(phase: &str) -> &str {
    match phase {
        "build_1" => "Opbouw 1",
        "build_2" => "Opbouw 2",
        "peak" => "Piek",
        "taper" => "Taper",
        "recovery" => "Herstel",
        _ => phase,
    }
}

/// Get session type label.
pub fn session_type_label(session_type: &str) -> &str {
    match session_type {
        "easy" => "Easy",
        "tempo" => "Tempo",
        "long" => "Lange duurloop",
        "interval" => "Interval",
        "rest" => "Rust",
        "cross" => "Crosstraining",
        "hike" => "Hike",
        "race" => "Race",
        "strength" => "Kracht",
        _ => session_type,
    }
}

/// Estimated pace (min/km) for a session type. Used for duration capping.
pub fn session_type_pace(session_type: &str) -> Option<f32> {
    match session_type {
        "easy" | "long" | "hike" => Some(6.5),
        "tempo" => Some(5.5),
        "interval" => Some(5.0),
        "race" => Some(5.5),
        _ => None,
    }
}
