use chrono::Local;
use uuid::Uuid;

use crate::models::{
    plan::{Day, Phase, Plan, SessionType, Week},
    profile::{Profile, RaceGoal, Terrain, Weekday},
};

/// Rotation of non-long session types, depending on goal
#[derive(Debug, Clone)]
struct SessionRotation(Vec<SessionType>);

impl SessionRotation {
    fn for_goal(goal: &RaceGoal, terrain: &Terrain) -> Self {
        let types = if goal.is_speed_goal() {
            vec![
                SessionType::Easy,
                SessionType::Tempo,
                SessionType::Interval,
                SessionType::Easy,
                SessionType::Tempo,
                SessionType::Cross,
            ]
        } else if goal.is_ultra() && *terrain == Terrain::Trail {
            vec![
                SessionType::Easy,
                SessionType::Tempo,
                SessionType::Hike,
                SessionType::Easy,
                SessionType::Interval,
                SessionType::Cross,
            ]
        } else if goal.is_ultra() {
            vec![
                SessionType::Easy,
                SessionType::Tempo,
                SessionType::Easy,
                SessionType::Interval,
                SessionType::Cross,
                SessionType::Easy,
            ]
        } else {
            vec![
                SessionType::Easy,
                SessionType::Interval,
                SessionType::Tempo,
                SessionType::Easy,
                SessionType::Cross,
                SessionType::Easy,
            ]
        };
        SessionRotation(types)
    }

    fn next(&self, idx: &mut usize) -> SessionType {
        let t = self.0[*idx % self.0.len()].clone();
        *idx += 1;
        t
    }
}

/// Phase definition: week range + base weekly km
struct PhaseDef {
    phase: Phase,
    start_week: u8,
    end_week: u8,
    base_km: f32,
}

pub fn generate_plan(profile: &Profile) -> Plan {
    let goal = &profile.race_goal;
    let race_km = goal.distance_km();

    // ── Number of weeks ──────────────────────────────────────────────────────
    let num_weeks = match profile.race_date {
        Some(date) => {
            let today = Local::now().date_naive();
            let days = (date - today).num_days().max(0);
            let weeks = (days / 7) as u8;
            weeks.clamp(goal.min_weeks(), goal.max_weeks())
        }
        None => 16u8.clamp(goal.min_weeks(), goal.max_weeks()),
    };

    // ── Scale factors ────────────────────────────────────────────────────────
    let km_scale = (profile.weekly_km / 55.0).clamp(0.5, 1.8);
    let age_factor = match profile.age {
        55.. => 0.85,
        50..=54 => 0.89,
        45..=49 => 0.93,
        40..=44 => 0.96,
        _ => 1.0,
    };
    let base_peak = goal.peak_km();

    // ── Phase split (25% / 25% / 25% / 25%) ─────────────────────────────────
    let p1 = (num_weeks as f32 * 0.25).round().max(2.0) as u8;
    let p2 = (num_weeks as f32 * 0.25).round().max(2.0) as u8;
    let p3 = (num_weeks as f32 * 0.25).round().max(2.0) as u8;
    let p4 = num_weeks - p1 - p2 - p3;

    let apply = |frac: f32| (base_peak * frac * km_scale * age_factor).round();

    let phases = vec![
        PhaseDef { phase: Phase::BuildOne, start_week: 1,          end_week: p1,          base_km: apply(0.70) },
        PhaseDef { phase: Phase::BuildTwo, start_week: p1 + 1,     end_week: p1 + p2,     base_km: apply(0.83) },
        PhaseDef { phase: Phase::Peak,     start_week: p1+p2+1,    end_week: p1+p2+p3,    base_km: apply(1.00) },
        PhaseDef { phase: Phase::Taper,    start_week: p1+p2+p3+1, end_week: num_weeks,   base_km: apply(0.64) },
    ];

    // ── Training day setup ────────────────────────────────────────────────────
    let mut training_days = profile.training_days.clone();
    training_days.sort();

    let long_day = profile.long_run_day;

    let long_frac = if goal.is_ultra() { 0.33 }
                    else if goal.is_marathon() { 0.30 }
                    else { 0.28 };

    let non_long_days: Vec<Weekday> = training_days.iter()
        .filter(|&&d| d != long_day)
        .copied()
        .collect();

    let rest_frac = (1.0 - long_frac) / non_long_days.len().max(1) as f32;

    // Build day-pattern map: for each weekday, what session type + km fraction
    let rotation = SessionRotation::for_goal(goal, &profile.terrain);
    let mut rot_idx = 0usize;

    #[derive(Clone)]
    struct DayPattern {
        session_type: SessionType,
        km_frac: f32,
    }

    let day_patterns: Vec<DayPattern> = (0u8..7).map(|i| {
        let day = Weekday(i);
        if !training_days.contains(&day) {
            return DayPattern { session_type: SessionType::Rest, km_frac: 0.0 };
        }
        if day == long_day {
            return DayPattern { session_type: SessionType::Long, km_frac: 0.0 };
        }
        let session_type = rotation.next(&mut rot_idx);
        let km_frac = match session_type {
            SessionType::Tempo    => rest_frac * 1.1,
            SessionType::Interval => rest_frac * 1.0,
            _                     => rest_frac * 0.9,
        };
        DayPattern { session_type, km_frac }
    }).collect();

    // ── Build weeks ───────────────────────────────────────────────────────────
    let mut weeks = Vec::with_capacity(num_weeks as usize);

    for w in 1..=num_weeks {
        let phase_def = phases.iter()
            .find(|p| w >= p.start_week && w <= p.end_week)
            .unwrap_or(phases.last().unwrap());

        let is_recovery = w % 4 == 0;
        let is_taper    = matches!(phase_def.phase, Phase::Taper);
        let is_race_week = w == num_weeks;

        let target_km = if is_race_week {
            (race_km * 0.4).round()
        } else if w == num_weeks - 1 {
            (phase_def.base_km * 0.72).round()
        } else if is_recovery {
            (phase_def.base_km * 0.70).round()
        } else if !is_taper {
            (phase_def.base_km * (1.0 + ((w % 4) as f32 - 1.0) * 0.05)).round()
        } else {
            phase_def.base_km
        };

        let days: Vec<Day> = (0u8..7).map(|i| {
            let pattern = &day_patterns[i as usize];

            if matches!(pattern.session_type, SessionType::Rest | SessionType::Cross) {
                return Day {
                    weekday: i,
                    session_type: pattern.session_type.clone(),
                    target_km: 0.0,
                    adjusted_km: None,
                    completed: false,
                    notes: None,
                    feedback: None,
                    strava_activity_id: None,
                };
            }

            let raw_km = if matches!(pattern.session_type, SessionType::Long) {
                target_km * long_frac
            } else {
                target_km * pattern.km_frac
            };

            // Cap by max duration if configured
            let capped_km = duration_cap(i, &pattern.session_type, raw_km, profile);

            // Recovery weeks: downgrade intensity
            let mut session_type = pattern.session_type.clone();
            if is_recovery && matches!(session_type, SessionType::Interval | SessionType::Tempo) {
                session_type = SessionType::Easy;
            }

            // Race day override
            if is_race_week && i == long_day.0 {
                return Day {
                    weekday: i,
                    session_type: SessionType::Race,
                    target_km: race_km,
                    adjusted_km: None,
                    completed: false,
                    notes: Some("Race dag! 🏆".into()),
                    feedback: None,
                    strava_activity_id: None,
                };
            }

            Day {
                weekday: i,
                session_type,
                target_km: capped_km.max(3.0),
                adjusted_km: None,
                completed: false,
                notes: None,
                feedback: None,
                strava_activity_id: None,
            }
        }).collect();

        weeks.push(Week {
            week_number: w,
            phase: phase_def.phase.clone(),
            is_recovery,
            target_km,
            original_target_km: target_km,
            week_adjustment: 1.0,
            days,
        });
    }

    Plan {
        id: Uuid::new_v4(),
        user_id: profile.user_id,
        weeks,
    }
}

/// Cap session km based on max configured duration for that day
fn duration_cap(day_idx: u8, session_type: &SessionType, raw_km: f32, profile: &Profile) -> f32 {
    let day = Weekday(day_idx);
    let Some(dur) = profile.max_duration_per_day.iter().find(|d| d.day == day) else {
        return raw_km;
    };
    let Some(pace) = session_type.pace_min_per_km() else {
        return raw_km;
    };
    let max_km = dur.max_minutes as f32 / pace;
    raw_km.min(max_km)
}
