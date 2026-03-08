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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_day(session_type: SessionType, target_km: f32, adjusted_km: Option<f32>) -> Day {
        Day {
            weekday: 0,
            session_type,
            target_km,
            adjusted_km,
            completed: false,
            notes: None,
            feedback: None,
            strava_activity_id: None,
        }
    }

    // ── Day ──────────────────────────────────────────────────────────────────

    #[test]
    fn effective_km_uses_adjusted_when_set() {
        let day = make_day(SessionType::Easy, 10.0, Some(8.0));
        assert_eq!(day.effective_km(), 8.0);
    }

    #[test]
    fn effective_km_falls_back_to_target() {
        let day = make_day(SessionType::Easy, 10.0, None);
        assert_eq!(day.effective_km(), 10.0);
    }

    #[test]
    fn effective_km_adjusted_zero_is_respected() {
        let day = make_day(SessionType::Easy, 10.0, Some(0.0));
        assert_eq!(day.effective_km(), 0.0);
    }

    // ── SessionType ──────────────────────────────────────────────────────────

    #[test]
    fn rest_and_cross_are_not_running() {
        assert!(!SessionType::Rest.is_running());
        assert!(!SessionType::Cross.is_running());
    }

    #[test]
    fn all_other_types_are_running() {
        for t in [
            SessionType::Easy, SessionType::Tempo, SessionType::Long,
            SessionType::Interval, SessionType::Hike, SessionType::Race,
        ] {
            assert!(t.is_running(), "{t:?} should be considered running");
        }
    }

    #[test]
    fn pace_is_none_for_non_running_types() {
        assert!(SessionType::Rest.pace_min_per_km().is_none());
        assert!(SessionType::Cross.pace_min_per_km().is_none());
        assert!(SessionType::Race.pace_min_per_km().is_none());
    }

    #[test]
    fn pace_is_some_for_running_types() {
        for t in [
            SessionType::Easy, SessionType::Tempo, SessionType::Long,
            SessionType::Interval, SessionType::Hike,
        ] {
            assert!(t.pace_min_per_km().is_some(), "{t:?} should have a pace");
            assert!(t.pace_min_per_km().unwrap() > 0.0, "{t:?} pace must be positive");
        }
    }

    #[test]
    fn tempo_is_faster_than_easy() {
        let tempo = SessionType::Tempo.pace_min_per_km().unwrap();
        let easy  = SessionType::Easy.pace_min_per_km().unwrap();
        assert!(tempo < easy, "tempo pace should be faster (lower min/km) than easy");
    }

    #[test]
    fn target_zones_not_empty_for_running() {
        for t in [
            SessionType::Easy, SessionType::Tempo, SessionType::Long,
            SessionType::Interval, SessionType::Hike, SessionType::Race,
        ] {
            assert!(!t.target_zones().is_empty(), "{t:?} should have target zones");
        }
    }

    #[test]
    fn rest_has_no_target_zones() {
        assert!(SessionType::Rest.target_zones().is_empty());
    }

    // ── Phase ────────────────────────────────────────────────────────────────

    #[test]
    fn phase_labels_are_unique() {
        let labels = [
            Phase::BuildOne.label(),
            Phase::BuildTwo.label(),
            Phase::Peak.label(),
            Phase::Taper.label(),
        ];
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(unique.len(), 4, "all phase labels should be unique");
    }

    #[test]
    fn phase_labels_are_nonempty() {
        for phase in [Phase::BuildOne, Phase::BuildTwo, Phase::Peak, Phase::Taper] {
            assert!(!phase.label().is_empty(), "{phase:?} label should not be empty");
        }
    }
}
