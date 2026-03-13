use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::models::profile::{
    Gender, HrZone, Profile, RaceGoal, RunningExperience, SleepCategory,
    Terrain, Weekday, PreviousUltra,
};

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

    /// Dutch question text for each step.
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
            Self::Summary => "", // Dynamically generated
            Self::Done => "",
        }
    }

    /// Returns quick reply options and input type for applicable steps.
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
                QuickReply { label: "5 km".into(), value: "five_km".into(), emoji: Some("⚡".into()) },
                QuickReply { label: "10 km".into(), value: "ten_km".into(), emoji: Some("🎯".into()) },
                QuickReply { label: "Halve marathon".into(), value: "half_marathon".into(), emoji: Some("🥈".into()) },
                QuickReply { label: "Marathon".into(), value: "marathon".into(), emoji: Some("🏆".into()) },
                QuickReply { label: "50 km".into(), value: "fifty_km".into(), emoji: Some("🦅".into()) },
                QuickReply { label: "100 km".into(), value: "hundred_km".into(), emoji: Some("🔥".into()) },
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
    pub gender: Option<Gender>,
    pub running_years: Option<RunningExperience>,
    pub weekly_km: Option<f32>,
    pub time_10k: Option<String>,
    pub race_goal: Option<RaceGoal>,
    pub race_date: Option<NaiveDate>,
    pub training_days: Vec<Weekday>,
    pub long_run_day: Option<Weekday>,
    pub rest_hr: Option<u16>,
    pub max_hr: Option<u16>,
    pub complaints: Option<String>,
}

impl IntakeState {
    pub fn new() -> Self {
        Self {
            step: IntakeStep::Welcome,
            name: None,
            date_of_birth: None,
            gender: None,
            running_years: None,
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

    /// Build a Profile from the collected intake data.
    pub fn to_profile(&self, user_id: Uuid) -> Option<Profile> {
        let rest_hr = self.rest_hr.unwrap_or(60);
        let max_hr = self.max_hr;
        let hr_zones = max_hr.map(|m| HrZone::calculate(m, rest_hr));

        Some(Profile {
            id: Uuid::new_v4(),
            user_id,
            name: self.name.clone()?,
            date_of_birth: self.date_of_birth?,
            gender: self.gender.clone()?,
            running_years: self.running_years.clone()?,
            weekly_km: self.weekly_km?,
            previous_ultra: PreviousUltra::None,
            time_10k: self.time_10k.clone(),
            time_half_marathon: None,
            time_marathon: None,
            race_goal: self.race_goal.clone()?,
            race_time_goal: None,
            race_date: self.race_date,
            terrain: Terrain::Road,
            training_days: self.training_days.clone(),
            strength_days: Vec::new(),
            max_duration_per_day: Vec::new(),
            long_run_day: self.long_run_day.unwrap_or(Weekday(5)),
            max_hr,
            rest_hr,
            hr_zones,
            sleep_hours: SleepCategory::SevenToEight,
            complaints: self.complaints.clone(),
            previous_injuries: Vec::new(),
        })
    }

    /// Build a summary string.
    fn summary(&self) -> String {
        let days_nl = ["Ma", "Di", "Wo", "Do", "Vr", "Za", "Zo"];
        let training_str: Vec<&str> = self.training_days
            .iter()
            .map(|d| *days_nl.get(d.0 as usize).unwrap_or(&"?"))
            .collect();
        let long_run = self.long_run_day
            .map(|d| days_nl.get(d.0 as usize).unwrap_or(&"?").to_string())
            .unwrap_or_else(|| "?".into());

        let goal_label = match &self.race_goal {
            Some(RaceGoal::FiveKm) => "5 km",
            Some(RaceGoal::TenKm) => "10 km",
            Some(RaceGoal::HalfMarathon) => "Halve marathon",
            Some(RaceGoal::Marathon) => "Marathon",
            Some(RaceGoal::FiftyKm) => "50 km",
            Some(RaceGoal::HundredKm) => "100 km",
            _ => "Onbekend",
        };

        format!(
            "📋 Hier is een samenvatting:\n\n\
             👤 Naam: {}\n\
             🎂 Geboortedatum: {}\n\
             ⚧ Geslacht: {}\n\
             🏃 Ervaring: {:?}\n\
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
            match &self.gender {
                Some(Gender::Male) => "Man",
                Some(Gender::Female) => "Vrouw",
                Some(Gender::Other) => "Anders",
                None => "?",
            },
            self.running_years.as_ref().map(|r| format!("{:?}", r)).unwrap_or_else(|| "?".into()),
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

/// Check if a user has an active intake session.
pub async fn has_active_intake(user_id: Uuid) -> bool {
    let map = INTAKE_STATES.lock().await;
    map.contains_key(&user_id)
}

/// Start a new intake session for a user, sending the welcome message.
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

/// Handle a user reply during intake. Returns true if intake is still in progress.
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

    // Validate and store the answer
    let validation = validate_and_store(user_id, current_step, value).await;

    match validation {
        ValidationResult::Ok => {
            // Advance to next step
            let next_step = {
                let mut map = INTAKE_STATES.lock().await;
                let state = map.get_mut(&user_id).unwrap();
                state.step = current_step.next();
                state.step
            };

            if next_step == IntakeStep::Summary {
                // Send summary
                let summary = {
                    let map = INTAKE_STATES.lock().await;
                    map.get(&user_id).unwrap().summary()
                };
                let _ = tx.send(StreamEvent::TextDelta { delta: summary }).await;
                let _ = tx.send(StreamEvent::MessageEnd).await;

                // Send summary quick replies
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
                // Build profile and generate plan
                return complete_intake(user_id, tx, agent).await;
            }

            send_step(user_id, tx).await?;
            Ok(true)
        }
        ValidationResult::Restart => {
            // Reset intake state
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
            // Re-send the same step's quick replies
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

/// Remove intake state for a user.
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
        IntakeStep::Welcome => {
            // Any response advances
            ValidationResult::Ok
        }
        IntakeStep::Name => {
            if value.is_empty() || value.len() < 2 {
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
                "male" | "man" => Gender::Male,
                "female" | "vrouw" => Gender::Female,
                "other" | "anders" => Gender::Other,
                _ => return ValidationResult::Error("Kies alsjeblieft een geslacht.".into()),
            };
            state.gender = Some(gender);
            ValidationResult::Ok
        }
        IntakeStep::Experience => {
            let exp = match value {
                "less_than_two_years" => RunningExperience::LessThanTwoYears,
                "two_to_five_years" => RunningExperience::TwoToFiveYears,
                "five_to_ten_years" => RunningExperience::FiveToTenYears,
                "more_than_ten_years" => RunningExperience::MoreThanTenYears,
                _ => return ValidationResult::Error("Kies een van de opties.".into()),
            };
            state.running_years = Some(exp);
            ValidationResult::Ok
        }
        IntakeStep::WeeklyKm => {
            match value.replace(',', ".").parse::<f32>() {
                Ok(km) if km >= 0.0 && km <= 300.0 => {
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
            let goal = match value {
                "five_km" => RaceGoal::FiveKm,
                "ten_km" => RaceGoal::TenKm,
                "half_marathon" => RaceGoal::HalfMarathon,
                "marathon" => RaceGoal::Marathon,
                "fifty_km" => RaceGoal::FiftyKm,
                "hundred_km" => RaceGoal::HundredKm,
                _ => return ValidationResult::Error("Kies een wedstrijddoel.".into()),
            };
            state.race_goal = Some(goal);
            ValidationResult::Ok
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
            // Value is comma-separated day indices like "0,1,3,5"
            let days: Vec<Weekday> = value
                .split(',')
                .filter_map(|s| s.trim().parse::<u8>().ok())
                .filter(|&d| d <= 6)
                .map(Weekday)
                .collect();

            if days.len() < 2 || days.len() > 7 {
                return ValidationResult::Error("Kies 2 tot 7 trainingsdagen.".into());
            }
            state.training_days = days;
            ValidationResult::Ok
        }
        IntakeStep::LongRunDay => {
            match value.parse::<u8>() {
                Ok(d) if d <= 6 => {
                    state.long_run_day = Some(Weekday(d));
                    ValidationResult::Ok
                }
                _ => ValidationResult::Error("Kies een dag (0=Ma tot 6=Zo).".into()),
            }
        }
        IntakeStep::HeartRate => {
            if value.to_lowercase() == "skip" {
                state.rest_hr = Some(60); // default
                ValidationResult::Ok
            } else {
                // May contain rest_hr or rest_hr,max_hr
                let parts: Vec<&str> = value.split(',').collect();
                match parts[0].trim().parse::<u16>() {
                    Ok(rhr) if rhr >= 30 && rhr <= 120 => {
                        state.rest_hr = Some(rhr);
                        if parts.len() > 1 {
                            if let Ok(mhr) = parts[1].trim().parse::<u16>() {
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
            // "confirm" or anything else proceeds
            ValidationResult::Ok
        }
        IntakeStep::Done => ValidationResult::Ok,
    }
}

/// Send the current step's question and quick replies.
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

    // Send question text
    let _ = tx.send(StreamEvent::TextDelta { delta: question }).await;
    let _ = tx.send(StreamEvent::MessageEnd).await;

    // Send quick replies if applicable
    if let Some((question_id, options, input_type)) = step.quick_replies() {
        let _ = tx.send(StreamEvent::QuickReplies {
            question_id,
            options,
            input_type: Some(input_type),
        }).await;
    }

    Ok(())
}

/// Complete the intake: build profile, generate plan, save to DB.
async fn complete_intake(
    user_id: Uuid,
    tx: &mpsc::Sender<StreamEvent>,
    agent: &CoachAgent,
) -> Result<bool, super::AgentError> {
    let profile = {
        let map = INTAKE_STATES.lock().await;
        let state = map.get(&user_id).unwrap();
        state.to_profile(user_id)
    };

    let Some(profile) = profile else {
        let _ = tx.send(StreamEvent::Error {
            message: "Er ontbreken gegevens. Probeer het opnieuw.".into(),
        }).await;
        clear_intake(user_id).await;
        return Ok(false);
    };

    // Send "generating plan" message
    let _ = tx.send(StreamEvent::TextDelta {
        delta: "Top! 🎉 Ik ga nu je persoonlijke trainingsplan genereren. Even geduld...".into(),
    }).await;
    let _ = tx.send(StreamEvent::MessageEnd).await;

    // Save profile
    let profile_id = crate::db::profiles::upsert(&agent.db, &profile)
        .await
        .map_err(|e| super::AgentError::Database(e))?;

    // Generate plan using the same AI flow as the regular endpoint
    let plan_result = generate_plan_for_intake(agent, &profile).await;

    match plan_result {
        Ok(plan) => {
            let race_date = profile.race_date;
            let race_goal = format!("{:?}", profile.race_goal);
            let num_weeks = plan.weeks.len();

            crate::db::plans::deactivate_all(&agent.db, user_id)
                .await
                .map_err(|e| super::AgentError::Database(e))?;
            crate::db::plans::insert(&agent.db, &plan, profile_id, race_date, &race_goal)
                .await
                .map_err(|e| super::AgentError::Database(e))?;

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
                plan_id: plan.id.to_string(),
                week: None,
            }).await;
        }
        Err(e) => {
            tracing::error!("Intake plan generation failed: {}", e);
            let _ = tx.send(StreamEvent::Error {
                message: format!("Plan generatie mislukt: {}. Probeer het later opnieuw.", e),
            }).await;
        }
    }

    clear_intake(user_id).await;
    Ok(false)
}

/// Generate plan for intake (reuses the same AI approach as the plans route).
async fn generate_plan_for_intake(
    agent: &CoachAgent,
    profile: &Profile,
) -> Result<crate::models::plan::Plan, anyhow::Error> {
    let profile_json = serde_json::to_string_pretty(profile)?;

    let prompt = format!(
        r#"Genereer een compleet trainingsschema voor de volgende hardloper. Antwoord ALLEEN met valid JSON, geen uitleg.

PROFIEL:
{profile_json}

Genereer een Plan object met het volgende JSON format (volg dit EXACT):
{{
  "id": "<random uuid>",
  "user_id": "{user_id}",
  "weeks": [
    {{
      "week_number": 1,
      "phase": "build_one",
      "is_recovery": false,
      "target_km": 25.0,
      "original_target_km": 25.0,
      "week_adjustment": 1.0,
      "days": [
        {{
          "weekday": 0,
          "session_type": "easy",
          "target_km": 6.0,
          "adjusted_km": null,
          "completed": false,
          "notes": "Rustige duurloop",
          "feedback": null,
          "strava_activity_id": null
        }}
      ]
    }}
  ]
}}

REGELS:
- Bereken het aantal weken tot de race datum ({race_date:?})
- Periodisering: Build I (40%) → Build II (30%) → Peak (15%) → Taper (15%)
- Elke 3-4 weken een recovery week (is_recovery=true, ~60% volume)
- Respecteer de trainingsdagen van de loper: {training_days:?}
- Lange duurloop op: {long_run_day:?}
- Progressieve overload: max 10% volume toename per week
- Rustdagen op niet-trainingsdagen (session_type="rest", target_km=0)
- Notes in het Nederlands
- id moet een geldige UUID v4 zijn
- user_id moet "{user_id}" zijn"#,
        profile_json = profile_json,
        user_id = profile.user_id,
        race_date = profile.race_date,
        training_days = profile.training_days,
        long_run_day = profile.long_run_day,
    );

    let response = agent.chat_single(&prompt).await?;

    // Extract JSON from response and sanitize enum values
    let json_str = extract_json_from_response(&response)?;
    let json_str = sanitize_plan_json(&json_str);
    let plan: crate::models::plan::Plan = serde_json::from_str(&json_str)?;

    Ok(plan)
}

/// Fix common AI mistakes in generated plan JSON — normalize enum values.
fn sanitize_plan_json(json: &str) -> String {
    json.replace("\"intervals\"", "\"interval\"")
        .replace("\"recovery\"", "\"easy\"")
        .replace("\"threshold\"", "\"tempo\"")
        .replace("\"long_run\"", "\"long\"")
        .replace("\"hill\"", "\"tempo\"")
        .replace("\"fartlek\"", "\"tempo\"")
        .replace("\"speed\"", "\"interval\"")
        .replace("\"shakeout\"", "\"easy\"")
        .replace("\"vo2max\"", "\"interval\"")
        .replace("\"vo2_max\"", "\"interval\"")
        .replace("\"warmup\"", "\"easy\"")
        .replace("\"warm_up\"", "\"easy\"")
        .replace("\"cooldown\"", "\"easy\"")
        .replace("\"cool_down\"", "\"easy\"")
        .replace("\"steady\"", "\"tempo\"")
        .replace("\"progression\"", "\"tempo\"")
        .replace("\"strides\"", "\"easy\"")
        .replace("\"sprint\"", "\"interval\"")
        .replace("\"build_1\"", "\"build_one\"")
        .replace("\"build_2\"", "\"build_two\"")
        .replace("\"build1\"", "\"build_one\"")
        .replace("\"build2\"", "\"build_two\"")
        .replace("\"tapering\"", "\"taper\"")
}

fn extract_json_from_response(text: &str) -> Result<String, anyhow::Error> {
    if let Some(start) = text.find("```json") {
        let content = &text[start + 7..];
        if let Some(end) = content.find("```") {
            return Ok(content[..end].trim().to_string());
        }
    }
    if let Some(start) = text.find("```") {
        let content = &text[start + 3..];
        if let Some(end) = content.find("```") {
            return Ok(content[..end].trim().to_string());
        }
    }
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].to_string());
        }
    }
    anyhow::bail!("No JSON found in AI response")
}

use chrono::Datelike;
