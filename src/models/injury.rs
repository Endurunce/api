use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Injury matching the `injuries` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Injury {
    pub id: Uuid,
    pub user_id: Uuid,
    pub locations: Vec<String>,
    pub severity: i16,
    pub can_walk: bool,
    pub can_run: bool,
    pub description: Option<String>,
    pub status: String,
    pub reported_at: NaiveDate,
    pub resolved_at: Option<NaiveDate>,
}

/// Input for reporting an injury.
#[derive(Debug, Deserialize)]
pub struct InjuryInput {
    pub locations: Vec<String>,
    pub severity: i16,
    pub can_walk: bool,
    pub can_run: bool,
    pub description: Option<String>,
}

/// Severity classification.
#[derive(Debug, Clone, PartialEq)]
pub enum SeverityClass {
    Mild,
    Moderate,
    Severe,
}

impl Injury {
    /// Classify the severity of the injury.
    pub fn severity_class(&self) -> SeverityClass {
        if !self.can_run {
            return SeverityClass::Severe;
        }
        match self.severity {
            1..=3 => SeverityClass::Mild,
            4..=6 => SeverityClass::Moderate,
            _ => SeverityClass::Severe,
        }
    }
}

/// Estimate recovery time in weeks based on severity.
pub fn estimated_recovery_weeks(injury: &Injury) -> u8 {
    match injury.severity_class() {
        SeverityClass::Mild => 1,
        SeverityClass::Moderate => 2,
        SeverityClass::Severe => 4,
    }
}
