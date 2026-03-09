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
    let age_factor = match profile.age_years() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::profile::*;
    use uuid::Uuid;

    fn base_profile() -> Profile {
        Profile {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "Test Runner".into(),
            age: 35,
            gender: Gender::Male,
            running_years: RunningExperience::TwoToFiveYears,
            weekly_km: 55.0,
            previous_ultra: PreviousUltra::None,
            time_10k: None,
            time_half_marathon: None,
            time_marathon: None,
            race_goal: RaceGoal::Marathon,
            race_date: None,
            terrain: Terrain::Road,
            training_days: vec![Weekday(0), Weekday(2), Weekday(4), Weekday(6)],
            max_duration_per_day: vec![],
            long_run_day: Weekday(6),
            max_hr: Some(185),
            rest_hr: 55,
            hr_zones: None,
            sleep_hours: SleepCategory::SevenToEight,
            complaints: None,
            previous_injuries: vec![],
        }
    }

    // ── Week count ───────────────────────────────────────────────────────────

    #[test]
    fn default_week_count_without_race_date() {
        // Marathon default = 16 (clamped between min 12 and max 24)
        let plan = generate_plan(&base_profile());
        assert_eq!(plan.weeks.len(), 16);
    }

    #[test]
    fn race_date_soon_clamps_to_min_weeks() {
        let mut profile = base_profile(); // Marathon min = 12
        let soon = chrono::Local::now().date_naive() + chrono::Duration::days(7);
        profile.race_date = Some(soon);
        let plan = generate_plan(&profile);
        assert_eq!(plan.weeks.len() as u8, RaceGoal::Marathon.min_weeks());
    }

    #[test]
    fn race_date_far_clamps_to_max_weeks() {
        let mut profile = base_profile(); // Marathon max = 24
        let far = chrono::Local::now().date_naive() + chrono::Duration::days(365 * 2);
        profile.race_date = Some(far);
        let plan = generate_plan(&profile);
        assert_eq!(plan.weeks.len() as u8, RaceGoal::Marathon.max_weeks());
    }

    // ── Structure ────────────────────────────────────────────────────────────

    #[test]
    fn week_numbers_are_sequential_from_1() {
        let plan = generate_plan(&base_profile());
        for (i, week) in plan.weeks.iter().enumerate() {
            assert_eq!(week.week_number, (i + 1) as u8);
        }
    }

    #[test]
    fn every_week_has_exactly_7_days() {
        let plan = generate_plan(&base_profile());
        for week in &plan.weeks {
            assert_eq!(week.days.len(), 7, "week {} has {} days", week.week_number, week.days.len());
        }
    }

    #[test]
    fn weekday_indices_are_0_through_6() {
        let plan = generate_plan(&base_profile());
        for week in &plan.weeks {
            for (i, day) in week.days.iter().enumerate() {
                assert_eq!(day.weekday, i as u8);
            }
        }
    }

    // ── Training days ─────────────────────────────────────────────────────────

    #[test]
    fn training_days_are_not_rest() {
        let profile = base_profile(); // Mon/Wed/Fri/Sun
        let plan = generate_plan(&profile);
        for week in plan.weeks.iter().take(plan.weeks.len() - 1) {
            for day in &week.days {
                if [0u8, 2, 4, 6].contains(&day.weekday) {
                    assert_ne!(
                        day.session_type, SessionType::Rest,
                        "training day {} in week {} should not be rest", day.weekday, week.week_number
                    );
                }
            }
        }
    }

    #[test]
    fn non_training_days_are_rest() {
        let profile = base_profile(); // Tue/Thu/Sat are rest
        let plan = generate_plan(&profile);
        for week in &plan.weeks {
            for day in &week.days {
                if [1u8, 3, 5].contains(&day.weekday) {
                    assert_eq!(
                        day.session_type, SessionType::Rest,
                        "non-training day {} in week {} should be rest", day.weekday, week.week_number
                    );
                }
            }
        }
    }

    #[test]
    fn long_run_day_has_long_session_in_regular_weeks() {
        let profile = base_profile(); // long day = Sunday (6)
        let plan = generate_plan(&profile);
        let non_race_weeks = plan.weeks.len() - 1;

        let long_count = plan.weeks.iter()
            .take(non_race_weeks)
            .filter(|w| !w.is_recovery)
            .filter(|w| w.days[6].session_type == SessionType::Long)
            .count();

        let expected = plan.weeks.iter()
            .take(non_race_weeks)
            .filter(|w| !w.is_recovery)
            .count();

        assert!(
            long_count >= expected / 2,
            "most non-recovery weeks should have Long on long day (got {long_count}/{expected})"
        );
    }

    // ── Recovery weeks ────────────────────────────────────────────────────────

    #[test]
    fn every_4th_week_is_recovery() {
        let plan = generate_plan(&base_profile());
        for week in &plan.weeks {
            if week.week_number % 4 == 0 {
                assert!(week.is_recovery, "week {} should be a recovery week", week.week_number);
            }
        }
    }

    #[test]
    fn non_4th_weeks_are_not_recovery() {
        let plan = generate_plan(&base_profile());
        for week in &plan.weeks {
            if week.week_number % 4 != 0 {
                assert!(!week.is_recovery, "week {} should not be recovery", week.week_number);
            }
        }
    }

    // ── Race week ─────────────────────────────────────────────────────────────

    #[test]
    fn last_week_contains_race_day() {
        let plan = generate_plan(&base_profile());
        let last = plan.weeks.last().unwrap();
        let has_race = last.days.iter().any(|d| d.session_type == SessionType::Race);
        assert!(has_race, "last week should contain a Race day");
    }

    #[test]
    fn race_day_km_matches_race_distance() {
        let profile = base_profile(); // Marathon = 42.2 km
        let plan = generate_plan(&profile);
        let last = plan.weeks.last().unwrap();
        let race_day = last.days.iter().find(|d| d.session_type == SessionType::Race).unwrap();
        assert_eq!(race_day.target_km, 42.2);
    }

    #[test]
    fn race_day_falls_on_long_run_day() {
        let profile = base_profile(); // long day = Sunday (6)
        let plan = generate_plan(&profile);
        let last = plan.weeks.last().unwrap();
        let race_day = last.days.iter().find(|d| d.session_type == SessionType::Race).unwrap();
        assert_eq!(race_day.weekday, profile.long_run_day.0);
    }

    // ── Volume ────────────────────────────────────────────────────────────────

    #[test]
    fn active_weeks_have_positive_km() {
        let plan = generate_plan(&base_profile());
        for week in plan.weeks.iter().take(plan.weeks.len() - 1) {
            assert!(week.target_km > 0.0, "week {} has 0 target km", week.week_number);
        }
    }

    #[test]
    fn running_days_have_minimum_3km() {
        let plan = generate_plan(&base_profile());
        for week in &plan.weeks {
            for day in &week.days {
                if day.session_type.is_running() && day.session_type != SessionType::Race {
                    assert!(
                        day.target_km >= 3.0,
                        "day {} in week {} has {:.1} km, expected >= 3.0",
                        day.weekday, week.week_number, day.target_km
                    );
                }
            }
        }
    }

    #[test]
    fn older_athlete_gets_reduced_volume() {
        let mut young = base_profile();
        young.age = 30;
        let mut old = base_profile();
        old.age = 60;

        let young_plan = generate_plan(&young);
        let old_plan = generate_plan(&old);

        let young_peak: f32 = young_plan.weeks.iter().map(|w| w.target_km).fold(0.0_f32, f32::max);
        let old_peak:   f32 = old_plan.weeks.iter().map(|w| w.target_km).fold(0.0_f32, f32::max);

        assert!(old_peak < young_peak, "older athlete should have lower peak km");
    }

    // ── Duration cap ──────────────────────────────────────────────────────────

    #[test]
    fn duration_cap_limits_session_km() {
        let mut profile = base_profile();
        // Cap Sunday (long day, 6.5 min/km) to 60 min → max 9.23 km
        profile.max_duration_per_day = vec![
            DayDuration { day: Weekday(6), max_minutes: 60 },
        ];
        let plan = generate_plan(&profile);

        let max_allowed = 60.0_f32 / 6.5 + 0.01;
        for week in &plan.weeks {
            let sunday = &week.days[6];
            if sunday.session_type == SessionType::Long {
                assert!(
                    sunday.target_km <= max_allowed,
                    "week {} long day km {:.1} exceeds cap {:.1}", week.week_number, sunday.target_km, max_allowed
                );
            }
        }
    }

    // ── Session variety ───────────────────────────────────────────────────────

    #[test]
    fn trail_ultra_plan_includes_hike_sessions() {
        let mut profile = base_profile();
        profile.race_goal = RaceGoal::HundredKm;
        profile.terrain = Terrain::Trail;
        let plan = generate_plan(&profile);

        let has_hike = plan.weeks.iter()
            .any(|w| w.days.iter().any(|d| d.session_type == SessionType::Hike));
        assert!(has_hike, "trail ultra plan should include Hike sessions");
    }

    #[test]
    fn speed_goal_includes_tempo_and_interval() {
        let mut profile = base_profile();
        profile.race_goal = RaceGoal::Sub3Marathon;
        let plan = generate_plan(&profile);

        let has_tempo    = plan.weeks.iter().any(|w| w.days.iter().any(|d| d.session_type == SessionType::Tempo));
        let has_interval = plan.weeks.iter().any(|w| w.days.iter().any(|d| d.session_type == SessionType::Interval));
        assert!(has_tempo,    "sub-3 marathon plan should have Tempo sessions");
        assert!(has_interval, "sub-3 marathon plan should have Interval sessions");
    }

    // ── Plan identity ─────────────────────────────────────────────────────────

    #[test]
    fn plan_is_assigned_to_correct_user() {
        let profile = base_profile();
        let plan = generate_plan(&profile);
        assert_eq!(plan.user_id, profile.user_id);
    }

    #[test]
    fn each_plan_gets_unique_id() {
        let profile = base_profile();
        let plan1 = generate_plan(&profile);
        let plan2 = generate_plan(&profile);
        assert_ne!(plan1.id, plan2.id);
    }
}
