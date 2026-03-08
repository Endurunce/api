use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::NaiveDate;

/// Runner profile — collected during intake flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub user_id: Uuid,

    // Personal
    pub name: String,
    pub age: u8,
    pub gender: Gender,

    // Experience
    pub running_years: RunningExperience,
    pub weekly_km: f32,
    pub previous_ultra: PreviousUltra,

    // Race times (optional)
    pub time_10k: Option<String>,
    pub time_half_marathon: Option<String>,
    pub time_marathon: Option<String>,

    // Race goal
    pub race_goal: RaceGoal,
    pub race_date: Option<NaiveDate>,
    pub terrain: Terrain,

    // Training schedule preferences
    pub training_days: Vec<Weekday>,      // 0=Mon..6=Sun
    pub max_duration_per_day: Vec<DayDuration>,
    pub long_run_day: Weekday,

    // Heart rate
    pub max_hr: Option<u16>,
    pub rest_hr: u16,
    pub hr_zones: Option<Vec<HrZone>>,

    // Health
    pub sleep_hours: SleepCategory,
    pub complaints: Option<String>,
    pub previous_injuries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Male,
    Female,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunningExperience {
    LessThanTwoYears,
    TwoToFiveYears,
    FiveToTenYears,
    MoreThanTenYears,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PreviousUltra {
    None,
    TwentyFiveKm,
    FiftyKm,
    HundredKmPlus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RaceGoal {
    FiveKm,
    TenKm,
    HalfMarathon,
    Marathon,
    Sub3Marathon,
    Sub4Marathon,
    FiftyKm,
    HundredKm,
    Custom { distance_km: f32 },
}

impl RaceGoal {
    pub fn distance_km(&self) -> f32 {
        match self {
            RaceGoal::FiveKm => 5.0,
            RaceGoal::TenKm => 10.0,
            RaceGoal::HalfMarathon => 21.1,
            RaceGoal::Marathon | RaceGoal::Sub3Marathon | RaceGoal::Sub4Marathon => 42.2,
            RaceGoal::FiftyKm => 50.0,
            RaceGoal::HundredKm => 100.0,
            RaceGoal::Custom { distance_km } => *distance_km,
        }
    }

    pub fn peak_km(&self) -> f32 {
        match self {
            RaceGoal::FiveKm => 50.0,
            RaceGoal::TenKm => 60.0,
            RaceGoal::HalfMarathon => 70.0,
            RaceGoal::Marathon => 80.0,
            RaceGoal::Sub3Marathon => 90.0,
            RaceGoal::Sub4Marathon => 80.0,
            RaceGoal::FiftyKm => 90.0,
            RaceGoal::HundredKm => 110.0,
            RaceGoal::Custom { distance_km } => (distance_km * 1.8).min(120.0),
        }
    }

    pub fn min_weeks(&self) -> u8 {
        match self {
            RaceGoal::FiveKm => 6,
            RaceGoal::TenKm => 8,
            RaceGoal::HalfMarathon => 10,
            RaceGoal::Marathon | RaceGoal::Sub3Marathon | RaceGoal::Sub4Marathon => 12,
            RaceGoal::FiftyKm => 14,
            RaceGoal::HundredKm => 16,
            RaceGoal::Custom { .. } => 12,
        }
    }

    pub fn max_weeks(&self) -> u8 {
        match self {
            RaceGoal::FiveKm => 12,
            RaceGoal::TenKm => 16,
            RaceGoal::HalfMarathon => 20,
            RaceGoal::Marathon | RaceGoal::Sub3Marathon | RaceGoal::Sub4Marathon => 24,
            RaceGoal::FiftyKm => 24,
            RaceGoal::HundredKm => 32,
            RaceGoal::Custom { .. } => 24,
        }
    }

    pub fn is_ultra(&self) -> bool {
        self.distance_km() >= 50.0
    }

    pub fn is_marathon(&self) -> bool {
        let km = self.distance_km();
        km >= 42.0 && km < 50.0
    }

    pub fn is_speed_goal(&self) -> bool {
        matches!(self, RaceGoal::Sub3Marathon | RaceGoal::Sub4Marathon)
    }
}

/// 0 = Monday, 6 = Sunday — serializes as a plain integer
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Weekday(pub u8);

impl serde::Serialize for Weekday {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Weekday {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(d)?;
        if v > 6 { return Err(serde::de::Error::custom("weekday must be 0-6")); }
        Ok(Weekday(v))
    }
}

impl Weekday {
    pub fn label(&self) -> &'static str {
        match self.0 {
            0 => "Ma", 1 => "Di", 2 => "Wo", 3 => "Do",
            4 => "Vr", 5 => "Za", 6 => "Zo", _ => "?",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayDuration {
    pub day: Weekday,
    pub max_minutes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HrZone {
    pub num: u8,
    pub name: String,
    pub lo: u16,
    pub hi: u16,
    pub color: String,
    pub description: String,
}

impl HrZone {
    /// Calculate 5 zones using the Karvonen method
    pub fn calculate(max_hr: u16, rest_hr: u16) -> Vec<HrZone> {
        let hrr = max_hr as f32 - rest_hr as f32;
        let kv = |f: f32| (rest_hr as f32 + hrr * f).round() as u16;

        vec![
            HrZone { num: 1, name: "Herstel".into(),          lo: kv(0.50), hi: kv(0.60), color: "#7bc67e".into(), description: "Actief herstel, wandelen".into() },
            HrZone { num: 2, name: "Aerobe basis".into(),     lo: kv(0.60), hi: kv(0.70), color: "#5a7a52".into(), description: "Lange duurlopen, praattempo".into() },
            HrZone { num: 3, name: "Aerobe drempel".into(),   lo: kv(0.70), hi: kv(0.80), color: "#c49a5a".into(), description: "Tempoduurloop, comfortabel".into() },
            HrZone { num: 4, name: "Anaerobe drempel".into(), lo: kv(0.80), hi: kv(0.90), color: "#b85c3a".into(), description: "Tempolopen, lactaatdrempel".into() },
            HrZone { num: 5, name: "VO₂max".into(),           lo: kv(0.90), hi: max_hr,   color: "#c0392b".into(), description: "Intervaltraining, max inspanning".into() },
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    Road,
    Trail,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SleepCategory {
    LessThanSix,
    SixToSeven,
    SevenToEight,
    MoreThanEight,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RaceGoal ─────────────────────────────────────────────────────────────

    #[test]
    fn race_goal_distances_correct() {
        assert_eq!(RaceGoal::FiveKm.distance_km(), 5.0);
        assert_eq!(RaceGoal::TenKm.distance_km(), 10.0);
        assert_eq!(RaceGoal::HalfMarathon.distance_km(), 21.1);
        assert_eq!(RaceGoal::Marathon.distance_km(), 42.2);
        assert_eq!(RaceGoal::Sub3Marathon.distance_km(), 42.2);
        assert_eq!(RaceGoal::Sub4Marathon.distance_km(), 42.2);
        assert_eq!(RaceGoal::FiftyKm.distance_km(), 50.0);
        assert_eq!(RaceGoal::HundredKm.distance_km(), 100.0);
        assert_eq!(RaceGoal::Custom { distance_km: 75.0 }.distance_km(), 75.0);
    }

    #[test]
    fn is_ultra_threshold_is_50km() {
        assert!(!RaceGoal::Marathon.is_ultra());
        assert!(!RaceGoal::HalfMarathon.is_ultra());
        assert!(!RaceGoal::Custom { distance_km: 49.9 }.is_ultra());
        assert!(RaceGoal::FiftyKm.is_ultra());
        assert!(RaceGoal::HundredKm.is_ultra());
        assert!(RaceGoal::Custom { distance_km: 50.0 }.is_ultra());
        assert!(RaceGoal::Custom { distance_km: 80.0 }.is_ultra());
    }

    #[test]
    fn is_marathon_range_42_to_50km() {
        assert!(RaceGoal::Marathon.is_marathon());
        assert!(RaceGoal::Sub3Marathon.is_marathon());
        assert!(RaceGoal::Sub4Marathon.is_marathon());
        assert!(!RaceGoal::HalfMarathon.is_marathon());
        assert!(!RaceGoal::FiftyKm.is_marathon());
        assert!(!RaceGoal::TenKm.is_marathon());
    }

    #[test]
    fn is_speed_goal_only_sub_variants() {
        assert!(!RaceGoal::Marathon.is_speed_goal());
        assert!(!RaceGoal::HalfMarathon.is_speed_goal());
        assert!(RaceGoal::Sub3Marathon.is_speed_goal());
        assert!(RaceGoal::Sub4Marathon.is_speed_goal());
    }

    #[test]
    fn min_weeks_never_exceeds_max_weeks() {
        let goals = [
            RaceGoal::FiveKm, RaceGoal::TenKm, RaceGoal::HalfMarathon,
            RaceGoal::Marathon, RaceGoal::Sub3Marathon, RaceGoal::Sub4Marathon,
            RaceGoal::FiftyKm, RaceGoal::HundredKm, RaceGoal::Custom { distance_km: 60.0 },
        ];
        for goal in &goals {
            assert!(
                goal.min_weeks() <= goal.max_weeks(),
                "{goal:?}: min_weeks {} > max_weeks {}", goal.min_weeks(), goal.max_weeks()
            );
        }
    }

    #[test]
    fn longer_race_needs_more_weeks() {
        assert!(RaceGoal::HundredKm.min_weeks() > RaceGoal::Marathon.min_weeks());
        assert!(RaceGoal::Marathon.min_weeks() > RaceGoal::HalfMarathon.min_weeks());
        assert!(RaceGoal::HalfMarathon.min_weeks() > RaceGoal::TenKm.min_weeks());
    }

    #[test]
    fn peak_km_scales_with_distance() {
        assert!(RaceGoal::HundredKm.peak_km() > RaceGoal::FiftyKm.peak_km());
        assert!(RaceGoal::FiftyKm.peak_km() > RaceGoal::Marathon.peak_km());
        assert!(RaceGoal::Marathon.peak_km() > RaceGoal::HalfMarathon.peak_km());
    }

    #[test]
    fn custom_goal_peak_km_capped_at_120() {
        let huge = RaceGoal::Custom { distance_km: 1000.0 };
        assert!(huge.peak_km() <= 120.0, "peak km should not exceed 120");
    }

    // ── HrZone ───────────────────────────────────────────────────────────────

    #[test]
    fn hr_zones_produces_five_zones() {
        let zones = HrZone::calculate(190, 55);
        assert_eq!(zones.len(), 5);
    }

    #[test]
    fn hr_zones_are_contiguous() {
        let zones = HrZone::calculate(190, 55);
        for i in 0..zones.len() - 1 {
            assert_eq!(
                zones[i].hi, zones[i + 1].lo,
                "zone {} hi should equal zone {} lo", i + 1, i + 2
            );
        }
    }

    #[test]
    fn hr_zone5_hi_equals_max_hr() {
        let zones = HrZone::calculate(190, 55);
        assert_eq!(zones[4].hi, 190);
    }

    #[test]
    fn hr_zones_lo_less_than_hi() {
        let zones = HrZone::calculate(185, 60);
        for zone in &zones {
            assert!(zone.lo < zone.hi, "zone {} lo should be less than hi", zone.num);
        }
    }

    #[test]
    fn hr_zone1_lo_matches_karvonen_50pct() {
        let max_hr = 190u16;
        let rest_hr = 55u16;
        let hrr = max_hr as f32 - rest_hr as f32;
        let expected_lo = (rest_hr as f32 + hrr * 0.50).round() as u16;
        let zones = HrZone::calculate(max_hr, rest_hr);
        assert_eq!(zones[0].lo, expected_lo);
    }

    #[test]
    fn hr_zones_numbered_1_to_5() {
        let zones = HrZone::calculate(185, 55);
        for (i, zone) in zones.iter().enumerate() {
            assert_eq!(zone.num, (i + 1) as u8);
        }
    }

    // ── Weekday ──────────────────────────────────────────────────────────────

    #[test]
    fn weekday_rejects_value_above_6() {
        let result: Result<Weekday, _> = serde_json::from_str("7");
        assert!(result.is_err(), "weekday 7 should be rejected");
        let result: Result<Weekday, _> = serde_json::from_str("255");
        assert!(result.is_err());
    }

    #[test]
    fn weekday_accepts_0_through_6() {
        for i in 0u8..=6 {
            let result: Result<Weekday, _> = serde_json::from_str(&i.to_string());
            assert!(result.is_ok(), "weekday {i} should be accepted");
            assert_eq!(result.unwrap().0, i);
        }
    }

    #[test]
    fn weekday_serializes_as_integer() {
        let wd = Weekday(4);
        let serialized = serde_json::to_string(&wd).unwrap();
        assert_eq!(serialized, "4");
    }

    #[test]
    fn weekday_ordering() {
        let mon = Weekday(0);
        let sun = Weekday(6);
        assert!(mon < sun);
        let mut days = vec![Weekday(6), Weekday(2), Weekday(0), Weekday(4)];
        days.sort();
        assert_eq!(days, vec![Weekday(0), Weekday(2), Weekday(4), Weekday(6)]);
    }
}
