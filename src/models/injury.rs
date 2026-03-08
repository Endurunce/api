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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use chrono::NaiveDate;

    fn injury(severity: u8, can_run: bool) -> InjuryReport {
        InjuryReport {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            reported_at: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            locations: vec![BodyLocation::Knee],
            severity,
            can_walk: true,
            can_run,
            description: None,
            recovery_status: RecoveryStatus::Active,
        }
    }

    #[test]
    fn severity_1_to_3_is_mild() {
        assert_eq!(injury(1, true).severity_class(), InjurySeverityClass::Mild);
        assert_eq!(injury(2, true).severity_class(), InjurySeverityClass::Mild);
        assert_eq!(injury(3, true).severity_class(), InjurySeverityClass::Mild);
    }

    #[test]
    fn severity_4_to_6_is_moderate() {
        assert_eq!(injury(4, true).severity_class(), InjurySeverityClass::Moderate);
        assert_eq!(injury(5, true).severity_class(), InjurySeverityClass::Moderate);
        assert_eq!(injury(6, true).severity_class(), InjurySeverityClass::Moderate);
    }

    #[test]
    fn severity_7_and_above_is_severe() {
        assert_eq!(injury(7, true).severity_class(), InjurySeverityClass::Severe);
        assert_eq!(injury(9, true).severity_class(), InjurySeverityClass::Severe);
        assert_eq!(injury(10, true).severity_class(), InjurySeverityClass::Severe);
    }

    #[test]
    fn cannot_run_is_always_severe_regardless_of_score() {
        assert_eq!(injury(1, false).severity_class(), InjurySeverityClass::Severe);
        assert_eq!(injury(3, false).severity_class(), InjurySeverityClass::Severe);
        assert_eq!(injury(6, false).severity_class(), InjurySeverityClass::Severe);
    }

    #[test]
    fn boundary_between_mild_and_moderate_is_4() {
        assert_eq!(injury(3, true).severity_class(), InjurySeverityClass::Mild);
        assert_eq!(injury(4, true).severity_class(), InjurySeverityClass::Moderate);
    }

    #[test]
    fn boundary_between_moderate_and_severe_is_7() {
        assert_eq!(injury(6, true).severity_class(), InjurySeverityClass::Moderate);
        assert_eq!(injury(7, true).severity_class(), InjurySeverityClass::Severe);
    }
}
