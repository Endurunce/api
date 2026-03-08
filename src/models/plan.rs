use serde::{Deserialize, Serialize};
use uuid::Uuid;
use super::feedback::Feedback;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: Uuid,
    pub user_id: Uuid,
    pub weeks: Vec<Week>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Week {
    pub week_number: u8,
    pub phase: Phase,
    pub is_recovery: bool,
    pub target_km: f32,
    pub original_target_km: f32,
    pub week_adjustment: f32,
    pub days: Vec<Day>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    BuildOne,
    BuildTwo,
    Peak,
    Taper,
}

impl Phase {
    pub fn label(&self) -> &'static str {
        match self {
            Phase::BuildOne => "Opbouw I",
            Phase::BuildTwo => "Opbouw II",
            Phase::Peak     => "Piek",
            Phase::Taper    => "Tapering",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Day {
    pub weekday: u8,       // 0=Mon..6=Sun
    pub session_type: SessionType,
    pub target_km: f32,
    pub adjusted_km: Option<f32>,
    pub completed: bool,
    pub notes: Option<String>,
    pub feedback: Option<Feedback>,
    pub strava_activity_id: Option<String>,
}

impl Day {
    pub fn effective_km(&self) -> f32 {
        self.adjusted_km.unwrap_or(self.target_km)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
    Easy,
    Tempo,
    Long,
    Interval,
    Hike,
    Rest,
    Cross,
    Race,
}

impl SessionType {
    pub fn label(&self) -> &'static str {
        match self {
            SessionType::Easy     => "Rustige duurloop",
            SessionType::Tempo    => "Tempoloup",
            SessionType::Long     => "Lange duurloop",
            SessionType::Interval => "Intervaltraining",
            SessionType::Hike     => "Wandel/Trail mix",
            SessionType::Rest     => "Rustdag",
            SessionType::Cross    => "Crosstraining",
            SessionType::Race     => "Race Day",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            SessionType::Easy     => "🌿",
            SessionType::Tempo    => "🔥",
            SessionType::Long     => "🏞️",
            SessionType::Interval => "⚡",
            SessionType::Hike     => "🥾",
            SessionType::Rest     => "☁️",
            SessionType::Cross    => "🚴",
            SessionType::Race     => "🏆",
        }
    }

    /// Minutes per km — used to convert max duration to km cap
    pub fn pace_min_per_km(&self) -> Option<f32> {
        match self {
            SessionType::Easy     => Some(6.5),
            SessionType::Tempo    => Some(5.0),
            SessionType::Long     => Some(6.5),
            SessionType::Interval => Some(5.5),
            SessionType::Hike     => Some(8.0),
            SessionType::Rest | SessionType::Cross | SessionType::Race => None,
        }
    }

    /// Target HR zones (zone numbers, 1-based)
    pub fn target_zones(&self) -> &'static [u8] {
        match self {
            SessionType::Easy | SessionType::Long | SessionType::Hike | SessionType::Cross => &[1, 2],
            SessionType::Tempo    => &[3],
            SessionType::Interval => &[4, 5],
            SessionType::Race     => &[2, 3],
            SessionType::Rest     => &[],
        }
    }

    pub fn is_running(&self) -> bool {
        !matches!(self, SessionType::Rest | SessionType::Cross)
    }
}
