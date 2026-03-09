use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::Claims,
    db,
    errors::{AppError, ApiResult},
    models::plan::SessionType,
    services::anthropic,
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionAdvice {
    pub goal: String,
    pub warmup: String,
    pub main_set: String,
    pub cooldown: String,
    pub summary: String,
    pub go_signal: String,
    pub stop_signal: String,
    pub too_hard: String,
    pub why_now: String,
}

/// GET /api/plans/:plan_id/weeks/:week/days/:weekday/advice
pub async fn session_advice(
    State(state): State<AppState>,
    claims: Claims,
    Path((plan_id, week_number, weekday)): Path<(Uuid, u8, u8)>,
) -> ApiResult<Json<SessionAdvice>> {
    let plan = db::plans::fetch_by_id(&state.db, plan_id, claims.sub)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Plan {} not found", plan_id)))?;

    let week = plan
        .weeks
        .iter()
        .find(|w| w.week_number == week_number)
        .ok_or_else(|| AppError::NotFound("Week niet gevonden".into()))?;

    let day = week
        .days
        .iter()
        .find(|d| d.weekday == weekday)
        .ok_or_else(|| AppError::NotFound("Dag niet gevonden".into()))?;

    let session_label = day.session_type.label();
    let target_km = day.effective_km();
    let phase_label = week.phase.label();
    let week_km = week.target_km;
    let notes_ctx = day.notes.as_deref().unwrap_or("");

    let profile_ctx = db::profiles::fetch_full_by_user(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?
        .unwrap_or_else(|| "Standaard hardloper".into());

    let prescription_line = if notes_ctx.is_empty() {
        String::new()
    } else {
        format!("Trainingsvoorschrift: {notes_ctx}\n")
    };

    let prompt = format!(
        "Genereer een exact trainingsvoorschrift voor deze sessie als JSON.\n\
        Sessie: {session_label}\n\
        Afstand: {target_km:.1} km\n\
        Fase: {phase_label}\n\
        Week {week_number} van {} (weekkilometrage: {week_km:.0} km)\n\
        {prescription_line}\
        Profiel: {profile_ctx}\n\
        \n\
        REGELS:\n\
        - Gebruik NOOIT bereiken zoals '6-8' of '3-4 min'. Kies altijd één exact getal.\n\
        - Als het trainingsvoorschrift hierboven staat, gebruik dan precies die aantallen/tijden.\n\
        - Geef ALLEEN valide JSON terug, geen markdown, geen uitleg buiten de JSON.\n\
        \n\
        {{\n\
          \"goal\": \"Doel in één zin\",\n\
          \"warmup\": \"Exacte inloop instructie met tijd/km\",\n\
          \"main_set\": \"Kern van de training met exact aantal herhalingen en duur, bijv. '6×4 min @ Z4'\",\n\
          \"cooldown\": \"Exacte uitloop instructie\",\n\
          \"summary\": \"Korte samenvatting bijv. '6×4 min Z4 · 2 min herstel'\",\n\
          \"go_signal\": \"Hoe een goede sessie voelt\",\n\
          \"stop_signal\": \"Stop als dit gebeurt\",\n\
          \"too_hard\": \"Aanpassing als het te zwaar is\",\n\
          \"why_now\": \"Waarom staat deze training nu in het schema\"\n\
        }}",
        plan.weeks.len(),
    );

    let messages = vec![anthropic::Message {
        role: "user".into(),
        content: prompt,
    }];

    match anthropic::complete(None, messages, 512).await {
        Ok(response) => {
            // Extract JSON from response
            let start = response.find('{').unwrap_or(0);
            let end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
            if let Ok(advice) = serde_json::from_str::<SessionAdvice>(&response[start..end]) {
                return Ok(Json(advice));
            }
        }
        Err(_) => {}
    }

    Ok(Json(fallback_advice(&day.session_type, target_km, week_number)))
}

/// POST /api/plans/:plan_id/weeks/:week/days/:weekday/uncomplete
pub async fn uncomplete_day(
    State(state): State<AppState>,
    claims: Claims,
    Path((plan_id, week_number, weekday)): Path<(Uuid, u8, u8)>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut plan = db::plans::fetch_by_id(&state.db, plan_id, claims.sub)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Plan {} not found", plan_id)))?;

    let week = plan
        .weeks
        .iter_mut()
        .find(|w| w.week_number == week_number)
        .ok_or_else(|| AppError::NotFound("Week niet gevonden".into()))?;

    let day = week
        .days
        .iter_mut()
        .find(|d| d.weekday == weekday)
        .ok_or_else(|| AppError::NotFound("Dag niet gevonden".into()))?;

    day.completed = false;
    day.feedback = None;

    db::plans::update_weeks(&state.db, plan_id, &plan.weeks).await?;

    Ok(Json(serde_json::json!({ "uncompleted": true })))
}

fn fallback_advice(session_type: &SessionType, km: f32, week: u8) -> SessionAdvice {
    match session_type {
        SessionType::Easy => SessionAdvice {
            goal: "Aerobe basis opbouwen en herstel bevorderen.".into(),
            warmup: "1 km rustig wandelen, dan geleidelijk optempo naar Z2.".into(),
            main_set: format!("{:.1} km op praattempo Z2. Je moet een volledig gesprek kunnen voeren.", (km - 1.0).max(1.0)),
            cooldown: "Laatste 500m uitlopen, 5 min stretchen.".into(),
            summary: format!("{:.1} km easy Z2", km),
            go_signal: "Ademhaling rustig, hartslag < 75% max, je kunt praten.".into(),
            stop_signal: "Pijn in gewrichten, hartslag > 85% of extreme vermoeidheid.".into(),
            too_hard: "Verlaag tempo met 30s/km of loop 10% minder.".into(),
            why_now: format!("Week {} bouwt de aerobe basis voor je race-voorbereiding.", week),
        },
        SessionType::Interval => SessionAdvice {
            goal: "VO2max verhogen en loopeconomie verbeteren via korte intensieve herhalingen.".into(),
            warmup: "2 km rustig inlopen, dan 4× 100m versnellingsloopje.".into(),
            main_set: format!("5× 1000m op 10k-racetempo. Herstel: 90s rustig joggen. Totaal: {:.1} km.", km),
            cooldown: "1.5 km rustig uitlopen.".into(),
            summary: "5× 1000m op 10k-tempo · 90s herstel".into(),
            go_signal: "Consistent tempo per herhaling, ademhaling herstelt binnen 90s.".into(),
            stop_signal: "Kniepijn, spierpijn of hartslag blijft > 95% na herstel.".into(),
            too_hard: "Verlaag naar 4 herhalingen of verleng het herstel naar 2 min.".into(),
            why_now: "Intervaltraining verhoogt je lactaatdrempel en loopeconomie.".into(),
        },
        SessionType::Tempo => SessionAdvice {
            goal: "Lactaatdrempel verhogen via comfortabele hoge intensiteit.".into(),
            warmup: "2 km rustig inlopen Z1-Z2.".into(),
            main_set: format!("{:.1} km tempolopen Z3. Stevig maar beheersbaar — 2 woorden per ademhaling mogelijk.", (km - 2.0).max(1.0)),
            cooldown: "1 km uitlopen Z1.".into(),
            summary: format!("{:.1} km tempo Z3", km),
            go_signal: "Ademhaling gecontroleerd, 2-3 woorden per zin mogelijk.".into(),
            stop_signal: "Pijn, of je kunt geen enkel woord meer zeggen.".into(),
            too_hard: "Verlaag naar Z2 of verminder met 1 km.".into(),
            why_now: "Tempolopen verhoogt je comfortabele racetempo.".into(),
        },
        SessionType::Long => SessionAdvice {
            goal: "Uithoudingsvermogen en vetverbranding opbouwen.".into(),
            warmup: "Eerste 2 km rustig Z1, dan geleidelijk naar Z2.".into(),
            main_set: format!("{:.1} km duurloop Z2. Neem elke 10 km een voedings-/watermoment.", (km - 2.0).max(1.0)),
            cooldown: "Laatste km rustig uitlopen, 10 min stretchen.".into(),
            summary: format!("{:.1} km lange duurloop Z2", km),
            go_signal: "Na de helft nog gelijkmatig tempo en gecontroleerde ademhaling.".into(),
            stop_signal: "Uitputting, hongerklop of spierkramp.".into(),
            too_hard: "Loop 10% minder of wissel in de laatste km's naar wandelen.".into(),
            why_now: "De lange duurloop is de pijler van je trainingsweek.".into(),
        },
        SessionType::Cross => SessionAdvice {
            goal: "Aerobe conditie behouden met minder belasting op de gewrichten.".into(),
            warmup: "5 min rustig opwarmen.".into(),
            main_set: "45 min fietsen, zwemmen of roeien Z1. Hartslag < 75% max.".into(),
            cooldown: "5 min rustig afkoelen.".into(),
            summary: "45 min crosstraining Z1".into(),
            go_signal: "Goed gevoel, geen pijn in de belaste gebieden.".into(),
            stop_signal: "Pijn in de geblesseerde zone.".into(),
            too_hard: "Korter of op lager tempo.".into(),
            why_now: "Crosstraining houdt de conditie op peil zonder extra loopbelasting.".into(),
        },
        SessionType::Hike => SessionAdvice {
            goal: "Benen versterken en terreinbehendigheid opbouwen.".into(),
            warmup: "Begin rustig, eerste 15 min als warming-up.".into(),
            main_set: format!("{:.1} km wandelen of trail. Focus op constante snelheid, ook bergop.", km),
            cooldown: "Laatste 10 min rustig neerlopen.".into(),
            summary: format!("{:.1} km hike/trail Z1-Z2", km),
            go_signal: "Gelijkmatig tempo, geen overmatige vermoeidheid in benen.".into(),
            stop_signal: "Enkelpijn of extreme spierpijn.".into(),
            too_hard: "Neem extra rustpauzes of verkort de route.".into(),
            why_now: "Hikesessies bouwen kracht en terreinervaring voor jouw race.".into(),
        },
        SessionType::Race => SessionAdvice {
            goal: "Maximale prestatie op racedag.".into(),
            warmup: "2 km rustig, dan oplopende intensiteit tot racetempo.".into(),
            main_set: format!("{:.1} km op doeltempo. Eerste 10% conservatief, dan negatieve split.", km),
            cooldown: "Geniet van het moment! Daarna 1 km rustig uitlopen.".into(),
            summary: format!("{:.1} km RACE 🏆", km),
            go_signal: "Controle in eerste helft, versnelling in tweede helft.".into(),
            stop_signal: "Ernstige pijn of desoriëntatie — stop direct.".into(),
            too_hard: "Zet een realistischer doeltempo in.".into(),
            why_now: "Dit is waar je al die weken naartoe hebt gewerkt.".into(),
        },
        SessionType::Rest => SessionAdvice {
            goal: "Herstel en adaptatie.".into(),
            warmup: "Geen opwarming nodig.".into(),
            main_set: "Rust! Eventueel lichte stretching of korte wandeling (< 30 min).".into(),
            cooldown: "Zorg voor goede slaap en hydratatie.".into(),
            summary: "Rustdag — herstel is training.".into(),
            go_signal: "Goed uitgerust gevoel.".into(),
            stop_signal: "Forceer geen extra training.".into(),
            too_hard: "Dit IS de aanpassing — rust is de training.".into(),
            why_now: "Rustdagen zijn waar de echte adaptatie plaatsvindt.".into(),
        },
    }
}
