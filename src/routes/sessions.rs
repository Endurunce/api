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
    pub good_signal: String,
    pub stop_signal: String,
    pub if_too_hard: String,
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

    let profile_ctx = db::profiles::fetch_full_by_user(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?
        .unwrap_or_else(|| "Standaard hardloper".into());

    let prompt = format!(
        "Genereer een exact trainingsvoorschrift voor deze sessie als JSON.\n\
        Sessie: {session_label}\nAfstand: {target_km:.1} km\n\
        Fase: {phase_label}\nWeek {week_number} van {} (weekkilometrage: {week_km:.0} km)\n\
        Profiel: {profile_ctx}\n\
        \n\
        Geef ALLEEN valide JSON terug, geen markdown, geen uitleg:\n\
        {{\n\
          \"goal\": \"Doel in één zin\",\n\
          \"warmup\": \"Exacte inloop instructie met tijd/km\",\n\
          \"main_set\": \"Kern van de training met exacte tempo/herhalingen\",\n\
          \"cooldown\": \"Exacte uitloop instructie\",\n\
          \"summary\": \"Korte samenvatting bijv. '6 × 1000m op 4:45 /km'\",\n\
          \"good_signal\": \"Hoe een goede sessie voelt\",\n\
          \"stop_signal\": \"Stop als dit gebeurt\",\n\
          \"if_too_hard\": \"Aanpassing als het te zwaar is\",\n\
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
            warmup: format!("1 km rustig wandelen, dan geleidelijk optempo lopen naar Z2."),
            main_set: format!("{:.1} km op praattempo (Zone 2). Je moet een volledig gesprek kunnen voeren.", (km - 1.0).max(1.0)),
            cooldown: "Laatste 500m uitlopen, 5 min stretchen.".into(),
            summary: format!("{:.1} km easy Z2", km),
            good_signal: "Ademhaling gecontroleerd, hartslag < 75% max, je kunt praten.".into(),
            stop_signal: "Pijn in gewrichten, hartslag > 85% of extreme vermoeidheid.".into(),
            if_too_hard: "Verlaag tempo met 30s/km of loop 10% minder km.".into(),
            why_now: format!("Week {} bouwt aerobe basis voor je race-voorbereiding.", week),
        },
        SessionType::Interval => SessionAdvice {
            goal: "VO2max verhogen en loopeconomie verbeteren via intensieve herhalingen.".into(),
            warmup: "2 km rustig inlopen, dan 4× 100m versnellingsloopje.".into(),
            main_set: format!("5× 1000m op 10k-racetempo. Herstel: 90s rustig joggen. Totaal: {:.1} km.", km),
            cooldown: "1-2 km rustig uitlopen.".into(),
            summary: "5 × 1000m op 10k-tempo · 90s herstel".into(),
            good_signal: "Consistent tempo per herhaling, gecontroleerde ademhaling na herstel.".into(),
            stop_signal: "Kniepijn, spierpijn of hartslag blijft > 95% na herstel.".into(),
            if_too_hard: "Verlaag naar 4 herhalingen of neem 2 min rust.".into(),
            why_now: "Intervaltraining deze week verhoogt je lactaatdrempel.".into(),
        },
        SessionType::Tempo => SessionAdvice {
            goal: "Lactaatdrempel omhoog brengen via comfortabele hoge intensiteit.".into(),
            warmup: "2 km rustig inlopen.".into(),
            main_set: format!("{:.1} km op tempoloooptempo (Zone 3–4). Stevig maar beheersbaar.", (km - 2.0).max(1.0)),
            cooldown: "1 km uitlopen.".into(),
            summary: format!("{:.1} km tempo Zone 3–4", km),
            good_signal: "Ademhaling stevig maar gecontroleerd, 2-3 woorden per zin mogelijk.".into(),
            stop_signal: "Pijn, of je kunt geen enkel woord meer zeggen.".into(),
            if_too_hard: "Verlaag naar Zone 3 of verminder met 1 km.".into(),
            why_now: "Tempolopen verhoogt je comfortabele racetempo.".into(),
        },
        SessionType::Long => SessionAdvice {
            goal: "Uithoudingsvermogen en vetverbranding opbouwen.".into(),
            warmup: "Eerste 2 km rustig, geleidelijk optempo.".into(),
            main_set: format!("{:.1} km duurloop op Z2. Neem elke 8-10 km water/voeding.", (km - 2.0).max(1.0)),
            cooldown: "Laatste km rustig uitlopen, 10 min stretchen.".into(),
            summary: format!("{:.1} km lange duurloop Z2", km),
            good_signal: "Na de helft nog steeds goed gevoel en gelijkmatig tempo.".into(),
            stop_signal: "Uitputting, hongerklop of spierkramp.".into(),
            if_too_hard: "Loop 10% minder of wandel de laatste km's.".into(),
            why_now: "De lange duurloop is de pijler van je week.".into(),
        },
        SessionType::Cross => SessionAdvice {
            goal: "Aerobe conditie behouden met minder belasting op gewrichten.".into(),
            warmup: "5 min rustig opwarmen.".into(),
            main_set: "45-60 min fietsen, zwemmen of roeien op Z2. Hartslag < 75% max.".into(),
            cooldown: "5 min rustig afkoelen.".into(),
            summary: "60 min crosstraining Z2".into(),
            good_signal: "Goed gevoel, geen pijn in getroffenen gebieden.".into(),
            stop_signal: "Pijn in de geblesseerde zone.".into(),
            if_too_hard: "Korter of lager tempo.".into(),
            why_now: "Crosstraining houdt de conditie op peil zonder extra loopbelasting.".into(),
        },
        SessionType::Hike => SessionAdvice {
            goal: "Benen versterken en terreinbehendigheid opbouwen.".into(),
            warmup: "Begin rustig, warm op in de eerste 15 min.".into(),
            main_set: format!("{:.1} km wandelen of trail-wandelen. Focus op negatieve splits.", km),
            cooldown: "Laatste 10 min rustig neerlopen.".into(),
            summary: format!("{:.1} km hike/trail", km),
            good_signal: "Gelijkmatig tempo, geen overmatige vermoeidheid.".into(),
            stop_signal: "Enkelpijn of extreme spierpijn.".into(),
            if_too_hard: "Neem meer rustpauzes of korter de route.".into(),
            why_now: "Hikesessies bouwen kracht en terreinervaring op voor jouw race.".into(),
        },
        SessionType::Race => SessionAdvice {
            goal: "Maximale prestatie op racedag.".into(),
            warmup: "2 km rustig, dan oplopende intensiteit tot racetempo.".into(),
            main_set: format!("{:.1} km op doeltempo. Eerste 10% conservatief, dan negatieve split.", km),
            cooldown: "Geniet van het moment! Daarna rustig uitlopen.".into(),
            summary: format!("{:.1} km RACE 🏆", km),
            good_signal: "Controle in eerste helft, versnelling in tweede helft.".into(),
            stop_signal: "Ernstige pijn of desorientatie.".into(),
            if_too_hard: "Zet een realistischer doeltempo.".into(),
            why_now: "Dit is waar je al die weken naartoe hebt gewerkt.".into(),
        },
        SessionType::Rest => SessionAdvice {
            goal: "Herstel en adaptatie.".into(),
            warmup: "Geen opwarming nodig.".into(),
            main_set: "Rust! Eventueel lichte stretching of korte wandeling (< 30 min).".into(),
            cooldown: "Zorg voor goede slaap en hydratatie.".into(),
            summary: "Rustdag".into(),
            good_signal: "Goed uitgerust gevoel.".into(),
            stop_signal: "Forceer geen training.".into(),
            if_too_hard: "Dit IS de aanpassing.".into(),
            why_now: "Rustdagen zijn waar de echte adaptatie plaatsvindt.".into(),
        },
    }
}
