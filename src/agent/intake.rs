use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::{CoachAgent, InputType, QuickReply, StreamEvent};

// ── Intake steps ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntakeStep {
    Welcome,
    Name,
    DateOfBirth,
    Gender,
    Experience,
    WeeklyKm,
    Performance,
    RaceGoal,
    RaceDate,
    TrainingDays,
    LongRunDay,
    HeartRate,
    Health,
    Summary,
    Done,
}

impl IntakeStep {
    fn next(self) -> Self {
        match self {
            Self::Welcome => Self::Name,
            Self::Name => Self::DateOfBirth,
            Self::DateOfBirth => Self::Gender,
            Self::Gender => Self::Experience,
            Self::Experience => Self::WeeklyKm,
            Self::WeeklyKm => Self::Performance,
            Self::Performance => Self::RaceGoal,
            Self::RaceGoal => Self::RaceDate,
            Self::RaceDate => Self::TrainingDays,
            Self::TrainingDays => Self::LongRunDay,
            Self::LongRunDay => Self::HeartRate,
            Self::HeartRate => Self::Health,
            Self::Health => Self::Summary,
            Self::Summary => Self::Done,
            Self::Done => Self::Done,
        }
    }

    fn question(&self) -> &'static str {
        match self {
            Self::Welcome => "Welkom bij EnduRunce! 🏃 Ik ben je persoonlijke AI-hardloopcoach. Laten we kennismaken zodat ik het perfecte trainingsplan voor je kan maken. Klaar om te beginnen?",
            Self::Name => "Hoe mag ik je noemen?",
            Self::DateOfBirth => "Wat is je geboortedatum?",
            Self::Gender => "Wat is je geslacht?",
            Self::Experience => "Hoeveel jaar loop je al?",
            Self::WeeklyKm => "Hoeveel kilometer loop je gemiddeld per week?",
            Self::Performance => "Heb je een recente 10 km tijd? (optioneel — typ 'skip' om over te slaan)",
            Self::RaceGoal => "Wat is je wedstrijddoel?",
            Self::RaceDate => "Wanneer is je wedstrijd? (bijv. 2026-10-15)",
            Self::TrainingDays => "Op welke dagen wil je trainen?",
            Self::LongRunDay => "Op welke dag wil je je lange duurloop doen?",
            Self::HeartRate => "Wat is je rusthartslag? (bijv. 55 — typ 'skip' als je het niet weet)",
            Self::Health => "Heb je klachten of blessures waar ik rekening mee moet houden? (typ 'nee' als je gezond bent)",
            Self::Summary | Self::Done => "",
        }
    }

    fn quick_replies(&self) -> Option<(String, Vec<QuickReply>, InputType)> {
        match self {
            Self::Welcome => Some(("welcome".into(), vec![
                QuickReply { label: "Laten we beginnen! 💪".into(), value: "start".into(), emoji: Some("💪".into()) },
            ], InputType::Chips)),
            Self::Name => Some(("name".into(), vec![], InputType::Text)),
            Self::DateOfBirth => Some(("date_of_birth".into(), vec![], InputType::DatePicker)),
            Self::Gender => Some(("gender".into(), vec![
                QuickReply { label: "Man".into(), value: "male".into(), emoji: Some("♂️".into()) },
                QuickReply { label: "Vrouw".into(), value: "female".into(), emoji: Some("♀️".into()) },
                QuickReply { label: "Anders".into(), value: "other".into(), emoji: None },
            ], InputType::Chips)),
            Self::Experience => Some(("experience".into(), vec![
                QuickReply { label: "< 2 jaar".into(), value: "less_than_two_years".into(), emoji: Some("🌱".into()) },
                QuickReply { label: "2–5 jaar".into(), value: "two_to_five_years".into(), emoji: Some("🌿".into()) },
                QuickReply { label: "5–10 jaar".into(), value: "five_to_ten_years".into(), emoji: Some("🌳".into()) },
                QuickReply { label: "10+ jaar".into(), value: "more_than_ten_years".into(), emoji: Some("🏔️".into()) },
            ], InputType::Chips)),
            Self::WeeklyKm => Some(("weekly_km".into(), vec![], InputType::Number)),
            Self::Performance => Some(("performance".into(), vec![
                QuickReply { label: "Overslaan".into(), value: "skip".into(), emoji: Some("⏭️".into()) },
            ], InputType::DurationPicker)),
            Self::RaceGoal => Some(("race_goal".into(), vec![
                QuickReply { label: "5 km".into(), value: "5k".into(), emoji: Some("⚡".into()) },
                QuickReply { label: "10 km".into(), value: "10k".into(), emoji: Some("🎯".into()) },
                QuickReply { label: "Halve marathon".into(), value: "half_marathon".into(), emoji: Some("🥈".into()) },
                QuickReply { label: "Marathon".into(), value: "marathon".into(), emoji: Some("🏆".into()) },
                QuickReply { label: "50 km".into(), value: "50k".into(), emoji: Some("🦅".into()) },
                QuickReply { label: "100 km".into(), value: "100k".into(), emoji: Some("🔥".into()) },
            ], InputType::Chips)),
            Self::RaceDate => Some(("race_date".into(), vec![], InputType::DatePicker)),
            Self::TrainingDays => Some(("training_days".into(), vec![
                QuickReply { label: "Ma".into(), value: "0".into(), emoji: None },
                QuickReply { label: "Di".into(), value: "1".into(), emoji: None },
                QuickReply { label: "Wo".into(), value: "2".into(), emoji: None },
                QuickReply { label: "Do".into(), value: "3".into(), emoji: None },
                QuickReply { label: "Vr".into(), value: "4".into(), emoji: None },
                QuickReply { label: "Za".into(), value: "5".into(), emoji: None },
                QuickReply { label: "Zo".into(), value: "6".into(), emoji: None },
            ], InputType::MultiChips)),
            Self::LongRunDay => Some(("long_run_day".into(), vec![
                QuickReply { label: "Ma".into(), value: "0".into(), emoji: None },
                QuickReply { label: "Di".into(), value: "1".into(), emoji: None },
                QuickReply { label: "Wo".into(), value: "2".into(), emoji: None },
                QuickReply { label: "Do".into(), value: "3".into(), emoji: None },
                QuickReply { label: "Vr".into(), value: "4".into(), emoji: None },
                QuickReply { label: "Za".into(), value: "5".into(), emoji: None },
                QuickReply { label: "Zo".into(), value: "6".into(), emoji: None },
            ], InputType::Chips)),
            Self::HeartRate => Some(("heart_rate".into(), vec![
                QuickReply { label: "Weet ik niet".into(), value: "skip".into(), emoji: Some("🤷".into()) },
            ], InputType::Number)),
            Self::Health => Some(("health".into(), vec![
                QuickReply { label: "Geen klachten ✅".into(), value: "nee".into(), emoji: Some("✅".into()) },
            ], InputType::Text)),
            Self::Summary => Some(("summary".into(), vec![
                QuickReply { label: "Ziet er goed uit! 🚀".into(), value: "confirm".into(), emoji: Some("🚀".into()) },
                QuickReply { label: "Opnieuw beginnen".into(), value: "restart".into(), emoji: Some("🔄".into()) },
            ], InputType::Chips)),
            Self::Done => None,
        }
    }
}

// ── Intake state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IntakeState {
    pub step: IntakeStep,
    pub name: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    pub running_experience: Option<String>,
    pub weekly_km: Option<f32>,
    pub time_10k: Option<String>,
    pub race_goal: Option<String>,
    pub race_date: Option<NaiveDate>,
    pub training_days: Vec<i16>,
    pub long_run_day: Option<i16>,
    pub rest_hr: Option<i16>,
    pub max_hr: Option<i16>,
    pub complaints: Option<String>,
}

impl IntakeState {
    pub fn new() -> Self {
        Self {
            step: IntakeStep::Welcome,
            name: None,
            date_of_birth: None,
            gender: None,
            running_experience: None,
            weekly_km: None,
            time_10k: None,
            race_goal: None,
            race_date: None,
            training_days: Vec::new(),
            long_run_day: None,
            rest_hr: None,
            max_hr: None,
            complaints: None,
        }
    }

    /// Build a ProfileInput from the collected intake data.
    pub fn to_profile_input(&self) -> Option<crate::models::profile::ProfileInput> {
        Some(crate::models::profile::ProfileInput {
            name: self.name.clone()?,
            date_of_birth: self.date_of_birth?,
            gender: self.gender.clone()?,
            running_experience: self.running_experience.clone(),
            weekly_km: self.weekly_km,
            time_5k: None,
            time_10k: self.time_10k.clone(),
            time_half: None,
            time_marathon: None,
            rest_hr: self.rest_hr,
            max_hr: self.max_hr,
            sleep_quality: None,
            complaints: self.complaints.clone(),
        })
    }

    /// Build a TrainingPreferencesInput.
    pub fn to_prefs_input(&self) -> crate::models::training_preferences::TrainingPreferencesInput {
        crate::models::training_preferences::TrainingPreferencesInput {
            training_days: if self.training_days.is_empty() {
                vec![1, 3, 5]
            } else {
                self.training_days.clone()
            },
            long_run_day: self.long_run_day,
            strength_days: None,
            max_duration_per_day: None,
            terrain: None,
        }
    }

    fn summary(&self) -> String {
        let days_nl = ["Ma", "Di", "Wo", "Do", "Vr", "Za", "Zo"];
        let training_str: Vec<&str> = self.training_days
            .iter()
            .filter_map(|&d| days_nl.get(d as usize).copied())
            .collect();
        let long_run = self.long_run_day
            .and_then(|d| days_nl.get(d as usize).copied())
            .unwrap_or("?");

        let goal_label = match self.race_goal.as_deref() {
            Some("5k") => "5 km",
            Some("10k") => "10 km",
            Some("half_marathon") => "Halve marathon",
            Some("marathon") => "Marathon",
            Some("50k") => "50 km",
            Some("100k") => "100 km",
            _ => "Onbekend",
        };

        let gender_label = match self.gender.as_deref() {
            Some("male") => "Man",
            Some("female") => "Vrouw",
            Some("other") => "Anders",
            _ => "?",
        };

        format!(
            "📋 Hier is een samenvatting:\n\n\
             👤 Naam: {}\n\
             🎂 Geboortedatum: {}\n\
             ⚧ Geslacht: {}\n\
             🏃 Ervaring: {}\n\
             📏 Wekelijks: {:.0} km\n\
             🎯 Doel: {}\n\
             📅 Wedstrijddatum: {}\n\
             🗓️ Trainingsdagen: {}\n\
             🏔️ Lange duurloop: {}\n\
             ❤️ Rusthart: {} bpm\n\
             🩺 Klachten: {}\n\n\
             Klopt dit? Dan ga ik je plan maken!",
            self.name.as_deref().unwrap_or("?"),
            self.date_of_birth.map(|d| d.to_string()).unwrap_or_else(|| "?".into()),
            gender_label,
            self.running_experience.as_deref().unwrap_or("?"),
            self.weekly_km.unwrap_or(0.0),
            goal_label,
            self.race_date.map(|d| d.to_string()).unwrap_or_else(|| "?".into()),
            training_str.join(", "),
            long_run,
            self.rest_hr.unwrap_or(0),
            self.complaints.as_deref().unwrap_or("Geen"),
        )
    }
}

// ── Global intake store ───────────────────────────────────────────────────────

static INTAKE_STATES: std::sync::LazyLock<Arc<Mutex<HashMap<Uuid, IntakeState>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

pub async fn has_active_intake(user_id: Uuid) -> bool {
    let map = INTAKE_STATES.lock().await;
    map.contains_key(&user_id)
}

pub async fn start_intake(
    user_id: Uuid,
    tx: &mpsc::Sender<StreamEvent>,
) -> Result<(), super::AgentError> {
    let state = IntakeState::new();
    {
        let mut map = INTAKE_STATES.lock().await;
        map.insert(user_id, state);
    }
    send_step(user_id, tx).await
}

pub async fn handle_reply(
    user_id: Uuid,
    value: &str,
    tx: &mpsc::Sender<StreamEvent>,
    agent: &CoachAgent,
) -> Result<bool, super::AgentError> {
    let current_step = {
        let map = INTAKE_STATES.lock().await;
        match map.get(&user_id) {
            Some(s) => s.step,
            None => return Ok(false),
        }
    };

    let validation = validate_and_store(user_id, current_step, value).await;

    match validation {
        ValidationResult::Ok => {
            let next_step = {
                let mut map = INTAKE_STATES.lock().await;
                let state = map.get_mut(&user_id).unwrap();
                state.step = current_step.next();
                state.step
            };

            if next_step == IntakeStep::Summary {
                let summary = {
                    let map = INTAKE_STATES.lock().await;
                    map.get(&user_id).unwrap().summary()
                };
                let _ = tx.send(StreamEvent::TextDelta { delta: summary }).await;
                let _ = tx.send(StreamEvent::MessageEnd).await;
                if let Some((qid, opts, itype)) = IntakeStep::Summary.quick_replies() {
                    let _ = tx.send(StreamEvent::QuickReplies {
                        question_id: qid,
                        options: opts,
                        input_type: Some(itype),
                    }).await;
                }
                return Ok(true);
            }

            if next_step == IntakeStep::Done {
                return complete_intake(user_id, tx, agent).await;
            }

            send_step(user_id, tx).await?;
            Ok(true)
        }
        ValidationResult::Restart => {
            {
                let mut map = INTAKE_STATES.lock().await;
                map.insert(user_id, IntakeState::new());
            }
            send_step(user_id, tx).await?;
            Ok(true)
        }
        ValidationResult::Error(msg) => {
            let _ = tx.send(StreamEvent::TextDelta { delta: msg }).await;
            let _ = tx.send(StreamEvent::MessageEnd).await;
            if let Some((qid, opts, itype)) = current_step.quick_replies() {
                let _ = tx.send(StreamEvent::QuickReplies {
                    question_id: qid,
                    options: opts,
                    input_type: Some(itype),
                }).await;
            }
            Ok(true)
        }
    }
}

pub async fn clear_intake(user_id: Uuid) {
    let mut map = INTAKE_STATES.lock().await;
    map.remove(&user_id);
}

// ── Internal helpers ──────────────────────────────────────────────────────────

enum ValidationResult {
    Ok,
    Restart,
    Error(String),
}

async fn validate_and_store(user_id: Uuid, step: IntakeStep, value: &str) -> ValidationResult {
    let value = value.trim();
    let mut map = INTAKE_STATES.lock().await;
    let state = match map.get_mut(&user_id) {
        Some(s) => s,
        None => return ValidationResult::Error("Geen actieve intake sessie.".into()),
    };

    match step {
        IntakeStep::Welcome => ValidationResult::Ok,
        IntakeStep::Name => {
            if value.len() < 2 {
                return ValidationResult::Error("Voer alsjeblieft een naam in (minimaal 2 tekens).".into());
            }
            state.name = Some(value.to_string());
            ValidationResult::Ok
        }
        IntakeStep::DateOfBirth => {
            match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
                Ok(date) => {
                    let today = chrono::Local::now().date_naive();
                    let age = today.year() - date.year();
                    if age < 16 || age > 100 {
                        return ValidationResult::Error("Je moet tussen de 16 en 100 jaar oud zijn.".into());
                    }
                    state.date_of_birth = Some(date);
                    ValidationResult::Ok
                }
                Err(_) => ValidationResult::Error("Gebruik het formaat JJJJ-MM-DD (bijv. 1990-06-15).".into()),
            }
        }
        IntakeStep::Gender => {
            let gender = match value.to_lowercase().as_str() {
                "male" | "man" => "male",
                "female" | "vrouw" => "female",
                "other" | "anders" => "other",
                _ => return ValidationResult::Error("Kies alsjeblieft een geslacht.".into()),
            };
            state.gender = Some(gender.to_string());
            ValidationResult::Ok
        }
        IntakeStep::Experience => {
            match value {
                "less_than_two_years" | "two_to_five_years" | "five_to_ten_years" | "more_than_ten_years" => {
                    state.running_experience = Some(value.to_string());
                    ValidationResult::Ok
                }
                _ => ValidationResult::Error("Kies een van de opties.".into()),
            }
        }
        IntakeStep::WeeklyKm => {
            match value.replace(',', ".").parse::<f32>() {
                Ok(km) if (0.0..=300.0).contains(&km) => {
                    state.weekly_km = Some(km);
                    ValidationResult::Ok
                }
                _ => ValidationResult::Error("Voer een geldig aantal kilometers in (0-300).".into()),
            }
        }
        IntakeStep::Performance => {
            if value.to_lowercase() == "skip" || value.is_empty() {
                state.time_10k = None;
            } else {
                state.time_10k = Some(value.to_string());
            }
            ValidationResult::Ok
        }
        IntakeStep::RaceGoal => {
            match value {
                "5k" | "10k" | "half_marathon" | "marathon" | "50k" | "100k" => {
                    state.race_goal = Some(value.to_string());
                    ValidationResult::Ok
                }
                // Support old format too
                "five_km" => { state.race_goal = Some("5k".into()); ValidationResult::Ok }
                "ten_km" => { state.race_goal = Some("10k".into()); ValidationResult::Ok }
                "fifty_km" => { state.race_goal = Some("50k".into()); ValidationResult::Ok }
                "hundred_km" => { state.race_goal = Some("100k".into()); ValidationResult::Ok }
                _ => ValidationResult::Error("Kies een wedstrijddoel.".into()),
            }
        }
        IntakeStep::RaceDate => {
            match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
                Ok(date) => {
                    let today = chrono::Local::now().date_naive();
                    if date <= today {
                        return ValidationResult::Error("De wedstrijddatum moet in de toekomst liggen.".into());
                    }
                    state.race_date = Some(date);
                    ValidationResult::Ok
                }
                Err(_) => ValidationResult::Error("Gebruik het formaat JJJJ-MM-DD (bijv. 2026-10-15).".into()),
            }
        }
        IntakeStep::TrainingDays => {
            let days: Vec<i16> = value
                .split(',')
                .filter_map(|s| s.trim().parse::<i16>().ok())
                .filter(|&d| d >= 0 && d <= 6)
                .collect();
            if days.len() < 2 || days.len() > 7 {
                return ValidationResult::Error("Kies 2 tot 7 trainingsdagen.".into());
            }
            state.training_days = days;
            ValidationResult::Ok
        }
        IntakeStep::LongRunDay => {
            match value.parse::<i16>() {
                Ok(d) if (0..=6).contains(&d) => {
                    state.long_run_day = Some(d);
                    ValidationResult::Ok
                }
                _ => ValidationResult::Error("Kies een dag (0=Ma tot 6=Zo).".into()),
            }
        }
        IntakeStep::HeartRate => {
            if value.to_lowercase() == "skip" {
                state.rest_hr = Some(60);
                ValidationResult::Ok
            } else {
                let parts: Vec<&str> = value.split(',').collect();
                match parts[0].trim().parse::<i16>() {
                    Ok(rhr) if (30..=120).contains(&rhr) => {
                        state.rest_hr = Some(rhr);
                        if parts.len() > 1 {
                            if let Ok(mhr) = parts[1].trim().parse::<i16>() {
                                if mhr > rhr && mhr <= 230 {
                                    state.max_hr = Some(mhr);
                                }
                            }
                        }
                        ValidationResult::Ok
                    }
                    _ => ValidationResult::Error("Voer een rusthart in tussen 30 en 120 bpm.".into()),
                }
            }
        }
        IntakeStep::Health => {
            if value.to_lowercase() == "nee" || value.is_empty() {
                state.complaints = None;
            } else {
                state.complaints = Some(value.to_string());
            }
            ValidationResult::Ok
        }
        IntakeStep::Summary => {
            if value == "restart" {
                return ValidationResult::Restart;
            }
            ValidationResult::Ok
        }
        IntakeStep::Done => ValidationResult::Ok,
    }
}

async fn send_step(
    user_id: Uuid,
    tx: &mpsc::Sender<StreamEvent>,
) -> Result<(), super::AgentError> {
    let (step, summary_text) = {
        let map = INTAKE_STATES.lock().await;
        let state = map.get(&user_id).unwrap();
        if state.step == IntakeStep::Summary {
            (state.step, Some(state.summary()))
        } else {
            (state.step, None)
        }
    };

    let question = if let Some(summary) = summary_text {
        summary
    } else {
        step.question().to_string()
    };

    let _ = tx.send(StreamEvent::TextDelta { delta: question }).await;
    let _ = tx.send(StreamEvent::MessageEnd).await;

    if let Some((question_id, options, input_type)) = step.quick_replies() {
        let _ = tx.send(StreamEvent::QuickReplies {
            question_id,
            options,
            input_type: Some(input_type),
        }).await;
    }

    Ok(())
}

/// Complete the intake: build profile + preferences, generate plan, save to DB.
async fn complete_intake(
    user_id: Uuid,
    tx: &mpsc::Sender<StreamEvent>,
    agent: &CoachAgent,
) -> Result<bool, super::AgentError> {
    let (profile_input, prefs_input, race_goal, race_date, weekly_km, training_days, long_run_day) = {
        let map = INTAKE_STATES.lock().await;
        let state = map.get(&user_id).unwrap();
        let pi = state.to_profile_input();
        let pr = state.to_prefs_input();
        let rg = state.race_goal.clone().unwrap_or_else(|| "marathon".into());
        let rd = state.race_date;
        let wk = state.weekly_km.unwrap_or(40.0);
        let td = state.training_days.clone();
        let lrd = state.long_run_day.unwrap_or(6);
        (pi, pr, rg, rd, wk, td, lrd)
    };

    let Some(profile_input) = profile_input else {
        let _ = tx.send(StreamEvent::Error {
            message: "Er ontbreken gegevens. Probeer het opnieuw.".into(),
        }).await;
        clear_intake(user_id).await;
        return Ok(false);
    };

    let _ = tx.send(StreamEvent::TextDelta {
        delta: "Top! 🎉 Ik ga nu je persoonlijke trainingsplan genereren. Even geduld...".into(),
    }).await;
    let _ = tx.send(StreamEvent::MessageEnd).await;

    // Save profile
    crate::db::profiles::upsert(&agent.db, user_id, &profile_input)
        .await
        .map_err(super::AgentError::Database)?;

    // Save training preferences
    crate::db::training_preferences::upsert(&agent.db, user_id, &prefs_input)
        .await
        .map_err(super::AgentError::Database)?;

    // Generate plan using schedule service
    let plan_insert = crate::services::schedule::generate_plan(
        user_id,
        &race_goal,
        race_date,
        None,
        "road",
        weekly_km,
        &training_days,
        long_run_day,
        &profile_input,
    );

    let plan_id = crate::db::plans::insert_full(&agent.db, &plan_insert)
        .await
        .map_err(super::AgentError::Database)?;

    let num_weeks = plan_insert.weeks.len();

    let _ = tx.send(StreamEvent::TextDelta {
        delta: format!(
            "Je trainingsplan is klaar! 🚀\n\n\
             📋 {} weken gepland\n\
             🎯 Doel: {}\n\n\
             Ga naar het 'Plan' tabblad om je schema te bekijken. Succes met trainen! 💪",
            num_weeks, race_goal
        ),
    }).await;
    let _ = tx.send(StreamEvent::MessageEnd).await;
    let _ = tx.send(StreamEvent::PlanUpdated {
        plan_id: plan_id.to_string(),
        week: None,
    }).await;

    clear_intake(user_id).await;
    Ok(false)
}

use chrono::Datelike;
