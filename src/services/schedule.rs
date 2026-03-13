use uuid::Uuid;

use crate::models::plan::{
    PlanInsert, SessionInsert, WeekInsert,
    race_goal_peak_km, race_goal_min_weeks, race_goal_max_weeks,
    is_marathon_or_longer,
};
use crate::models::profile::ProfileInput;

/// Generate a full plan using algorithmic plan generation.
/// Returns a `PlanInsert` ready to be inserted via `db::plans::insert_full`.
pub fn generate_plan(
    user_id: Uuid,
    race_goal: &str,
    race_date: Option<chrono::NaiveDate>,
    race_time_goal: Option<&str>,
    terrain: &str,
    weekly_km: f32,
    training_days: &[i16],
    long_run_day: i16,
    _profile: &ProfileInput,
) -> PlanInsert {
    // Calculate number of weeks
    let num_weeks = if let Some(rd) = race_date {
        let today = chrono::Local::now().date_naive();
        let days_until = (rd - today).num_days().max(0) as u16;
        let weeks = (days_until / 7) as u8;
        weeks
            .max(race_goal_min_weeks(race_goal))
            .min(race_goal_max_weeks(race_goal))
    } else {
        race_goal_min_weeks(race_goal) + 4
    };

    let peak_km = race_goal_peak_km(race_goal);
    let start_km = weekly_km.max(10.0);

    // Phase distribution
    let build1_end = (num_weeks as f32 * 0.35).ceil() as u8;
    let build2_end = (num_weeks as f32 * 0.65).ceil() as u8;
    let peak_end = (num_weeks as f32 * 0.85).ceil() as u8;
    // taper = rest

    let mut weeks = Vec::with_capacity(num_weeks as usize);

    for w in 1..=num_weeks {
        let phase = if w <= build1_end {
            "build_1"
        } else if w <= build2_end {
            "build_2"
        } else if w <= peak_end {
            "peak"
        } else {
            "taper"
        };

        // Recovery week every 4th week
        let is_recovery = w > 1 && w % 4 == 0 && phase != "taper";

        // Calculate target km with progressive overload
        let progress = w as f32 / num_weeks as f32;
        let mut target_km = if phase == "taper" {
            let taper_progress = (w - peak_end) as f32 / (num_weeks - peak_end) as f32;
            peak_km * (1.0 - taper_progress * 0.4) // Taper down to 60%
        } else {
            start_km + (peak_km - start_km) * progress
        };

        if is_recovery {
            target_km *= 0.6;
        }

        // Ensure no more than 10% increase from previous week
        if let Some(prev) = weeks.last() {
            let prev: &WeekInsert = prev;
            if !is_recovery && target_km > prev.target_km * 1.15 {
                target_km = prev.target_km * 1.10;
            }
        }

        let sessions = distribute_sessions(
            target_km,
            training_days,
            long_run_day,
            phase,
            is_recovery,
            is_marathon_or_longer(race_goal),
        );

        weeks.push(WeekInsert {
            week_number: w as i16,
            phase: phase.to_string(),
            target_km,
            is_recovery,
            notes: if is_recovery {
                Some("Herstelweek — verlaagd volume".into())
            } else {
                None
            },
            sessions,
        });
    }

    PlanInsert {
        user_id,
        race_goal: race_goal.to_string(),
        race_goal_km: None,
        race_time_goal: race_time_goal.map(|s| s.to_string()),
        race_date,
        terrain: terrain.to_string(),
        start_km,
        weeks,
    }
}

/// Distribute weekly km across training days.
fn distribute_sessions(
    target_km: f32,
    training_days: &[i16],
    long_run_day: i16,
    phase: &str,
    is_recovery: bool,
    is_marathon_plus: bool,
) -> Vec<SessionInsert> {
    let mut sessions = Vec::new();

    // Create sessions for all 7 days
    for weekday in 0..7i16 {
        let is_training = training_days.contains(&weekday);
        let is_long = weekday == long_run_day && is_training;

        if !is_training {
            sessions.push(SessionInsert {
                weekday,
                session_type: "rest".into(),
                target_km: 0.0,
                target_duration_min: None,
                target_hr_zones: None,
                notes: None,
                sort_order: weekday,
            });
            continue;
        }

        let (session_type, km_fraction, notes) = if is_long {
            let fraction = if is_marathon_plus { 0.30 } else { 0.25 };
            ("long", fraction, Some("Lange duurloop Z2".to_string()))
        } else if !is_recovery && should_be_quality_session(phase, weekday, training_days, long_run_day) {
            match phase {
                "build_2" | "peak" => {
                    let remaining_quality = training_days.iter()
                        .filter(|&&d| d != long_run_day && should_be_quality_session(phase, d, training_days, long_run_day))
                        .count();
                    if remaining_quality > 0 {
                        if weekday == *training_days.iter()
                            .filter(|&&d| d != long_run_day && should_be_quality_session(phase, d, training_days, long_run_day))
                            .next().unwrap_or(&weekday) 
                        {
                            ("tempo", 0.15, Some("Tempolopen Z3".to_string()))
                        } else {
                            ("interval", 0.12, Some("Intervallen Z4-Z5".to_string()))
                        }
                    } else {
                        ("easy", 0.15, Some("Rustige duurloop Z2".to_string()))
                    }
                }
                _ => ("tempo", 0.15, Some("Tempolopen Z3".to_string())),
            }
        } else {
            let non_long_count = training_days.iter().filter(|&&d| d != long_run_day).count();
            let fraction = if non_long_count > 0 {
                let quality_count = if !is_recovery {
                    training_days.iter()
                        .filter(|&&d| d != long_run_day && should_be_quality_session(phase, d, training_days, long_run_day))
                        .count()
                } else { 0 };
                let easy_count = non_long_count - quality_count;
                if easy_count > 0 {
                    (1.0 - 0.30 - 0.15 * quality_count as f32) / easy_count as f32
                } else {
                    0.15
                }
            } else {
                0.15
            };
            ("easy", fraction.max(0.1), Some("Rustige duurloop Z2".to_string()))
        };

        let km = (target_km * km_fraction).max(2.0);

        sessions.push(SessionInsert {
            weekday,
            session_type: session_type.into(),
            target_km: (km * 10.0).round() / 10.0,
            target_duration_min: None,
            target_hr_zones: match session_type {
                "easy" | "long" => Some(vec![1, 2]),
                "tempo" => Some(vec![3]),
                "interval" => Some(vec![4, 5]),
                _ => None,
            },
            notes,
            sort_order: weekday,
        });
    }

    sessions
}

/// Determine if a training day should be a quality session.
fn should_be_quality_session(phase: &str, weekday: i16, training_days: &[i16], long_run_day: i16) -> bool {
    if weekday == long_run_day {
        return false;
    }

    // In build_1, only 1 quality session. In build_2/peak, 2.
    let quality_count = match phase {
        "build_1" => 1,
        "build_2" | "peak" => 2,
        "taper" => 1,
        _ => 0,
    };

    if quality_count == 0 {
        return false;
    }

    // Pick the first N non-long training days as quality
    let quality_days: Vec<i16> = training_days
        .iter()
        .copied()
        .filter(|&d| d != long_run_day)
        .take(quality_count)
        .collect();

    quality_days.contains(&weekday)
}
