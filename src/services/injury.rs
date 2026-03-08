use crate::models::{
    injury::{InjuryReport, InjurySeverityClass},
    plan::{Plan, SessionType, Week},
};

/// Adapt a plan based on an injury report.
/// Returns the modified plan weeks from the current week onward.
pub fn adapt_plan_for_injury(plan: &mut Plan, injury: &InjuryReport, current_week: u8) {
    let severity = injury.severity_class();

    for week in plan.weeks.iter_mut().filter(|w| w.week_number >= current_week) {
        match severity {
            InjurySeverityClass::Mild => apply_mild(week),
            InjurySeverityClass::Moderate => apply_moderate(week),
            InjurySeverityClass::Severe => apply_severe(week),
        }

        // Recovery weeks come sooner after injury
        if week.week_number == current_week {
            week.is_recovery = true;
        }

        recalculate_week_km(week);
    }
}

/// Mild: reduce overall volume 15%, keep structure
fn apply_mild(week: &mut Week) {
    week.target_km *= 0.85;
    week.week_adjustment = 0.85;

    for day in week.days.iter_mut() {
        if day.session_type.is_running() {
            day.target_km = (day.target_km * 0.85).max(3.0);
        }
    }
}

/// Moderate: reduce 30%, convert intervals/tempo to easy or cross
fn apply_moderate(week: &mut Week) {
    week.target_km *= 0.70;
    week.week_adjustment = 0.70;

    for day in week.days.iter_mut() {
        match day.session_type {
            SessionType::Interval | SessionType::Tempo => {
                day.session_type = SessionType::Easy;
                day.target_km = (day.target_km * 0.70).max(3.0);
            }
            SessionType::Long => {
                day.target_km = (day.target_km * 0.60).max(3.0);
            }
            ref t if t.is_running() => {
                day.target_km = (day.target_km * 0.70).max(3.0);
            }
            _ => {}
        }
    }
}

/// Severe: replace all running with cross-training or rest
fn apply_severe(week: &mut Week) {
    week.target_km = 0.0;
    week.week_adjustment = 0.0;

    for day in week.days.iter_mut() {
        if day.session_type.is_running() {
            day.session_type = SessionType::Cross;
            day.target_km = 0.0;
            day.notes = Some("Vervangen door crosstraining i.v.m. blessure".into());
        }
    }
}

fn recalculate_week_km(week: &mut Week) {
    week.target_km = week.days.iter().map(|d| d.target_km).sum();
}

/// Estimate recovery time in weeks based on injury severity
pub fn estimated_recovery_weeks(injury: &InjuryReport) -> u8 {
    match injury.severity_class() {
        InjurySeverityClass::Mild     => 1,
        InjurySeverityClass::Moderate => 2,
        InjurySeverityClass::Severe   => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        injury::{BodyLocation, InjuryReport, RecoveryStatus},
        plan::{Day, Phase, Plan, SessionType, Week},
    };
    use uuid::Uuid;
    use chrono::NaiveDate;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_injury(severity: u8, can_run: bool) -> InjuryReport {
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

    fn make_day(weekday: u8, session_type: SessionType, target_km: f32) -> Day {
        Day {
            weekday,
            session_type,
            target_km,
            adjusted_km: None,
            completed: false,
            notes: None,
            feedback: None,
            strava_activity_id: None,
        }
    }

    fn make_week(number: u8) -> Week {
        // A realistic training week: Mon Easy, Wed Tempo, Fri Interval, Sun Long
        Week {
            week_number: number,
            phase: Phase::BuildOne,
            is_recovery: false,
            target_km: 50.0,
            original_target_km: 50.0,
            week_adjustment: 1.0,
            days: vec![
                make_day(0, SessionType::Easy,     10.0), // Mon
                make_day(1, SessionType::Rest,      0.0), // Tue
                make_day(2, SessionType::Tempo,     8.0), // Wed
                make_day(3, SessionType::Rest,      0.0), // Thu
                make_day(4, SessionType::Interval,  7.0), // Fri
                make_day(5, SessionType::Rest,      0.0), // Sat
                make_day(6, SessionType::Long,     15.0), // Sun
            ],
        }
    }

    fn make_plan(num_weeks: u8) -> Plan {
        Plan {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            weeks: (1..=num_weeks).map(make_week).collect(),
        }
    }

    // ── estimated_recovery_weeks ──────────────────────────────────────────────

    #[test]
    fn recovery_1_week_for_mild() {
        assert_eq!(estimated_recovery_weeks(&make_injury(2, true)), 1);
        assert_eq!(estimated_recovery_weeks(&make_injury(3, true)), 1);
    }

    #[test]
    fn recovery_2_weeks_for_moderate() {
        assert_eq!(estimated_recovery_weeks(&make_injury(4, true)), 2);
        assert_eq!(estimated_recovery_weeks(&make_injury(6, true)), 2);
    }

    #[test]
    fn recovery_4_weeks_for_severe() {
        assert_eq!(estimated_recovery_weeks(&make_injury(8, true)), 4);
        assert_eq!(estimated_recovery_weeks(&make_injury(1, false)), 4); // can't run
    }

    // ── adapt_plan_for_injury — mild ─────────────────────────────────────────

    #[test]
    fn mild_reduces_day_km_by_15_percent() {
        let mut plan = make_plan(2);
        adapt_plan_for_injury(&mut plan, &make_injury(2, true), 1);

        for week in &plan.weeks {
            let easy_day = &week.days[0]; // weekday 0, originally 10 km
            let expected = (10.0_f32 * 0.85).max(3.0);
            assert!(
                (easy_day.target_km - expected).abs() < 0.01,
                "easy day km {:.2} should be {:.2}", easy_day.target_km, expected
            );

            let long_day = &week.days[6]; // weekday 6, originally 15 km
            let expected = (15.0_f32 * 0.85).max(3.0);
            assert!(
                (long_day.target_km - expected).abs() < 0.01,
                "long day km {:.2} should be {:.2}", long_day.target_km, expected
            );
        }
    }

    #[test]
    fn mild_preserves_session_types() {
        let mut plan = make_plan(1);
        adapt_plan_for_injury(&mut plan, &make_injury(2, true), 1);

        let week = &plan.weeks[0];
        assert_eq!(week.days[0].session_type, SessionType::Easy);
        assert_eq!(week.days[2].session_type, SessionType::Tempo);
        assert_eq!(week.days[4].session_type, SessionType::Interval);
        assert_eq!(week.days[6].session_type, SessionType::Long);
    }

    // ── adapt_plan_for_injury — moderate ─────────────────────────────────────

    #[test]
    fn moderate_converts_interval_and_tempo_to_easy() {
        let mut plan = make_plan(2);
        adapt_plan_for_injury(&mut plan, &make_injury(5, true), 1);

        for week in &plan.weeks {
            for day in &week.days {
                assert_ne!(day.session_type, SessionType::Interval,
                    "Interval should be gone after moderate injury (week {})", week.week_number);
                assert_ne!(day.session_type, SessionType::Tempo,
                    "Tempo should be gone after moderate injury (week {})", week.week_number);
            }
        }
    }

    #[test]
    fn moderate_reduces_long_run_by_40_percent() {
        let mut plan = make_plan(1);
        adapt_plan_for_injury(&mut plan, &make_injury(5, true), 1);

        let long_day = &plan.weeks[0].days[6]; // originally 15 km
        let expected = (15.0_f32 * 0.60).max(3.0);
        assert!(
            (long_day.target_km - expected).abs() < 0.01,
            "long day km {:.2} should be {:.2}", long_day.target_km, expected
        );
    }

    // ── adapt_plan_for_injury — severe ───────────────────────────────────────

    #[test]
    fn severe_replaces_running_with_cross() {
        let mut plan = make_plan(2);
        adapt_plan_for_injury(&mut plan, &make_injury(8, true), 1);

        for week in &plan.weeks {
            for day in &week.days {
                assert!(
                    !day.session_type.is_running(),
                    "day {} should not be running after severe injury (week {})",
                    day.weekday, week.week_number
                );
            }
        }
    }

    #[test]
    fn severe_sets_all_km_to_zero() {
        let mut plan = make_plan(1);
        adapt_plan_for_injury(&mut plan, &make_injury(8, true), 1);

        let week = &plan.weeks[0];
        assert_eq!(week.target_km, 0.0);
        for day in &week.days {
            assert_eq!(day.target_km, 0.0, "day {} km should be 0 after severe injury", day.weekday);
        }
    }

    #[test]
    fn cannot_run_treated_as_severe() {
        let mut plan = make_plan(1);
        adapt_plan_for_injury(&mut plan, &make_injury(1, false), 1); // low severity but can't run

        for day in &plan.weeks[0].days {
            assert!(!day.session_type.is_running());
        }
    }

    // ── week targeting ───────────────────────────────────────────────────────

    #[test]
    fn only_affects_weeks_from_current_week_onward() {
        let mut plan = make_plan(4);
        let original_week1_km = plan.weeks[0].target_km;
        let original_week2_km = plan.weeks[1].target_km;

        adapt_plan_for_injury(&mut plan, &make_injury(8, true), 3);

        // Weeks 1 and 2 must be untouched
        assert_eq!(plan.weeks[0].target_km, original_week1_km, "week 1 should not change");
        assert_eq!(plan.weeks[1].target_km, original_week2_km, "week 2 should not change");

        // Week 3 (index 2) should be zeroed by severe
        assert_eq!(plan.weeks[2].target_km, 0.0, "week 3 should be zeroed");
    }

    #[test]
    fn current_week_is_marked_as_recovery() {
        let mut plan = make_plan(3);
        assert!(!plan.weeks[1].is_recovery, "week 2 should not be recovery before injury");

        adapt_plan_for_injury(&mut plan, &make_injury(2, true), 2);

        assert!(plan.weeks[1].is_recovery, "week 2 should be marked recovery after injury");
        assert!(!plan.weeks[0].is_recovery, "week 1 should not be affected");
    }

    #[test]
    fn week_adjustment_factor_set_correctly() {
        let mut plan = make_plan(1);

        adapt_plan_for_injury(&mut plan, &make_injury(2, true), 1); // mild
        assert!((plan.weeks[0].week_adjustment - 0.85).abs() < 0.01);

        let mut plan = make_plan(1);
        adapt_plan_for_injury(&mut plan, &make_injury(5, true), 1); // moderate
        assert!((plan.weeks[0].week_adjustment - 0.70).abs() < 0.01);

        let mut plan = make_plan(1);
        adapt_plan_for_injury(&mut plan, &make_injury(8, true), 1); // severe
        assert_eq!(plan.weeks[0].week_adjustment, 0.0);
    }
}
