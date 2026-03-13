use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    agent::CoachAgent,
    auth::Claims,
    db,
    errors::{AppError, ApiResult},
    models::profile::Profile,
    services::schedule::generate_plan as generate_plan_legacy,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct GeneratePlanRequest {
    pub profile: Profile,
}

#[derive(Debug, Serialize)]
pub struct GeneratePlanResponse {
    pub plan_id: Uuid,
    pub num_weeks: usize,
    pub plan: crate::models::plan::Plan,
}

/// POST /api/plans/generate — generate a new personalized training plan using AI.
///
/// **Auth:** Bearer JWT required. User must be ≥ 16 years old.
///
/// **Request body:** `{ "profile": Profile }` with race goal, training days, etc.
///
/// The AI coach agent analyzes the full profile and generates a periodized plan
/// with appropriate phases (build, peak, taper), session types, and weekly km targets.
/// Falls back to the legacy algorithm if AI generation fails.
///
/// **Response:** 201 with `{ plan_id, num_weeks, plan }`. Deactivates any previous plan.
pub async fn generate(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<GeneratePlanRequest>,
) -> ApiResult<(StatusCode, Json<GeneratePlanResponse>)> {
    let mut profile = req.profile;
    profile.user_id = claims.sub;
    profile.id = Uuid::new_v4();

    if profile.age_years() < 16 {
        return Err(AppError::BadRequest(
            "Je moet minimaal 16 jaar oud zijn om deze app te gebruiken.".into(),
        ));
    }

    // Try AI-generated plan first, fall back to legacy algorithm
    let plan = match generate_plan_ai(&state, &profile).await {
        Ok(plan) => {
            tracing::info!("AI generated plan for user {}", claims.sub);
            plan
        }
        Err(e) => {
            tracing::warn!("AI plan generation failed, falling back to legacy: {}", e);
            generate_plan_legacy(&profile)
        }
    };

    let profile_id = db::profiles::upsert(&state.db, &profile).await?;
    let race_date = profile.race_date;
    let race_goal = format!("{:?}", profile.race_goal);

    db::plans::deactivate_all(&state.db, profile.user_id).await?;
    db::plans::insert(&state.db, &plan, profile_id, race_date, &race_goal).await?;

    let num_weeks = plan.weeks.len();
    Ok((StatusCode::CREATED, Json(GeneratePlanResponse { plan_id: plan.id, num_weeks, plan })))
}

/// Use the AI coach agent to generate a training plan from a profile.
async fn generate_plan_ai(
    state: &AppState,
    profile: &Profile,
) -> Result<crate::models::plan::Plan, anyhow::Error> {
    let agent = CoachAgent::new(
        state.db.clone(),
        state.config.clone(),
        state.http.clone(),
    );

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
      "phase": "build_one",   // "build_one" | "build_two" | "peak" | "taper"
      "is_recovery": false,
      "target_km": 25.0,
      "original_target_km": 25.0,
      "week_adjustment": 1.0,
      "days": [
        {{
          "weekday": 0,          // 0=maandag .. 6=zondag
          "session_type": "easy", // "easy" | "tempo" | "long" | "interval" | "hike" | "rest" | "cross" | "race"
          "target_km": 6.0,
          "adjusted_km": null,
          "completed": false,
          "notes": "Rustige duurloop, houd het ontspannen",
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
- Krachttraining dagen: {strength_days:?}
- Progressieve overload: max 10% volume toename per week
- Rustdagen op niet-trainingsdagen (session_type="rest", target_km=0)
- Notes in het Nederlands, kort en specifiek per sessie
- Pas het niveau aan op basis van ervaring ({experience_years} jaar) en huidige wekelijkse km ({weekly_km} km)
- Houd rekening met blessuregeschiedenis: {health_notes:?}
- id moet een geldige UUID v4 zijn
- user_id moet "{user_id}" zijn"#,
        profile_json = profile_json,
        user_id = profile.user_id,
        race_date = profile.race_date,
        training_days = profile.training_days,
        long_run_day = profile.long_run_day,
        strength_days = profile.strength_days,
        experience_years = format!("{:?}", profile.running_years),
        weekly_km = profile.weekly_km,
        health_notes = profile.complaints.as_deref().unwrap_or("geen"),
    );

    let response = agent.chat_single(&prompt).await?;

    // Extract JSON from response (might be wrapped in ```json ... ```)
    let json_str = extract_json(&response)?;
    let json_str = sanitize_plan_json(&json_str);
    let plan: crate::models::plan::Plan = serde_json::from_str(&json_str)?;

    Ok(plan)
}

/// Extract JSON from a response that might contain markdown code blocks.
fn extract_json(text: &str) -> Result<String, anyhow::Error> {
    // Try to find ```json ... ``` block
    if let Some(start) = text.find("```json") {
        let content = &text[start + 7..];
        if let Some(end) = content.find("```") {
            return Ok(content[..end].trim().to_string());
        }
    }
    // Try to find ``` ... ``` block
    if let Some(start) = text.find("```") {
        let content = &text[start + 3..];
        if let Some(end) = content.find("```") {
            return Ok(content[..end].trim().to_string());
        }
    }
    // Try raw JSON (starts with {)
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].to_string());
        }
    }
    anyhow::bail!("No JSON found in AI response")
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

/// GET /api/plans — returns the authenticated user's active training plan.
///
/// **Auth:** Bearer JWT required.
///
/// **Response:** 200 with the full `Plan` JSON, or 404 if no active plan exists.
pub async fn get_active(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<crate::models::plan::Plan>> {
    db::plans::fetch_active(&state.db, claims.sub)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound("No active plan found".into()))
}

/// GET /api/plans/:plan_id — returns a specific plan by ID (scoped to the authenticated user).
///
/// **Auth:** Bearer JWT required. Returns 404 if the plan belongs to another user.
///
/// **Response:** 200 with the full `Plan` JSON.
pub async fn get_by_id(
    State(state): State<AppState>,
    claims: Claims,
    Path(plan_id): Path<Uuid>,
) -> ApiResult<Json<crate::models::plan::Plan>> {
    db::plans::fetch_by_id(&state.db, plan_id, claims.sub)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("Plan {} not found", plan_id)))
}
