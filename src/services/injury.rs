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
