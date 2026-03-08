use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::NaiveDate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjuryReport {
    pub id: Uuid,
    pub user_id: Uuid,
    pub reported_at: NaiveDate,

    pub locations: Vec<BodyLocation>,
    pub severity: u8,           // 1-10
    pub can_walk: bool,
    pub can_run: bool,
    pub description: Option<String>,

    pub recovery_status: RecoveryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BodyLocation {
    Knee,
    Achilles,
    Shin,
    Hip,
    Hamstring,
    Calf,
    Foot,
    Ankle,
    LowerBack,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStatus {
    Active,
    Recovering,
    Resolved,
}

/// Injury severity classification used for plan adaptation
#[derive(Debug, Clone, PartialEq)]
pub enum InjurySeverityClass {
    /// Severity 1-3: keep running, reduce intensity
    Mild,
    /// Severity 4-6: run/walk, significant reduction
    Moderate,
    /// Severity 7-10 or cannot run: stop running
    Severe,
}

impl InjuryReport {
    pub fn severity_class(&self) -> InjurySeverityClass {
        if !self.can_run || self.severity >= 7 {
            InjurySeverityClass::Severe
        } else if self.severity >= 4 {
            InjurySeverityClass::Moderate
        } else {
            InjurySeverityClass::Mild
        }
    }
}
