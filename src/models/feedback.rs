use serde::{Deserialize, Serialize};
use super::injury::InjuryReport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// Perceived exertion / feeling: 1 (very hard) to 5 (easy)
    pub feeling: u8,
    /// Pain during session
    pub pain: bool,
    pub notes: Option<String>,
    pub injury: Option<InjuryReport>,
    pub ai_advice: Option<AiAdvice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAdvice {
    pub message: String,
    pub alert_level: AlertLevel,
    pub adjustment: Option<PlanAdjustment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    Green,
    Yellow,
    Red,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanAdjustment {
    pub reduce_km_percent: Option<f32>,
    pub convert_to_cross: bool,
    pub insert_rest_days: u8,
    pub message: String,
}
