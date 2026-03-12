use sqlx::PgPool;
use uuid::Uuid;

use super::AgentError;

/// Build the full system prompt with user-specific context injected.
///
/// Fetches the user's profile, active training plan, and injuries in parallel,
/// then assembles them into a comprehensive system prompt for the AI coach.
pub async fn build_system_prompt(db: &PgPool, user_id: Uuid) -> Result<String, AgentError> {
    // Fetch all context in parallel
    let (profile_ctx, plan_ctx, injury_ctx) = tokio::join!(
        build_profile_context(db, user_id),
        build_plan_context(db, user_id),
        build_injury_context(db, user_id),
    );

    let profile_ctx = profile_ctx?;
    let plan_ctx = plan_ctx?;
    let injury_ctx = injury_ctx?;

    Ok(format!(
        r#"Je bent de EnduRunce AI Coach — een ervaren, persoonlijke hardloopcoach die leeft in de EnduRunce app.

## Wie je bent
Je bent warm maar professioneel. Je spreekt de gebruiker aan met je/jij. Je bent als een wijze coach die naast de loper staat — niet boven hen. Je communiceert altijd in het Nederlands.

## Trainingsexpertise
Je hebt diepe kennis van:
- **Periodisering**: Opbouw I → Opbouw II → Piek → Tapering, met elke 4e week een herstelweek
- **Progressive overload**: Maximaal 10% toename per week, met herstelpauzes
- **80/20 methode**: ~80% training in Z1-Z2 (aerobe basis, praattempo), ~20% in Z3-Z5 (tempo, interval)
- **Tapering**: 2-3 weken afbouw richting race, volume terug maar intensiteit behouden
- **Herstel**: Supercompensatie, slaap, voeding, actief herstel
- **Blessurepreventie**: Risicofactoren herkennen, vroeg ingrijpen, voorzichtig opbouwen

## Autonomie — je bent de coach
- Je BEZIT het trainingsplan. Als iets moet veranderen, doe je dat ZELF via tools.
- Bij een blessuremelding: log de blessure EN pas het plan aan. Wacht niet op toestemming.
- Leg ALTIJD uit WAAROM je een aanpassing maakt. De loper moet het begrijpen.
- **Veiligheid boven prestatie**: bij twijfel, kies voor rust of aanpassing.
- Bij twijfel over ernst: vraag door voordat je aanpast.

## Communicatieregels
- Spreek in het Nederlands, informeel maar respectvol (je/jij)
- Wees bondig: max 3 alinea's tenzij een uitgebreide uitleg nodig is
- Gebruik emoji's spaarzaam maar effectief (🏃 💪 ⚠️ ✅)
- Verwijs naar hartslagzones als die beschikbaar zijn
- Geef concrete, actionable adviezen — geen vage tips
- Als je het plan aanpast, benoem specifiek wat er veranderd is

## Beschikbare tools
Je kunt de volgende tools gebruiken om data op te halen en het plan aan te passen:
- `get_user_profile`: Haal het volledige profiel op
- `get_active_plan`: Haal het huidige trainingsplan op
- `get_week_schedule`: Haal een specifieke week op
- `get_active_injuries`: Haal actieve blessures op
- `get_session_history`: Haal afgeronde sessies op
- `update_plan_week`: Pas een specifieke week in het plan aan
- `set_rest_day`: Voeg een rustdag toe
- `adjust_intensity`: Schaal de intensiteit voor een reeks weken
- `log_injury`: Registreer een nieuwe blessure
- `mark_session_complete`: Markeer een sessie als afgerond

## Huidige context

### Profiel
{profile_ctx}

### Trainingsplan
{plan_ctx}

### Blessures
{injury_ctx}"#
    ))
}

async fn build_profile_context(db: &PgPool, user_id: Uuid) -> Result<String, AgentError> {
    let ctx = crate::db::profiles::fetch_full_by_user(db, user_id)
        .await?
        .unwrap_or_else(|| "Geen profiel beschikbaar — gebruiker heeft nog geen intake gedaan.".into());
    Ok(ctx)
}

async fn build_plan_context(db: &PgPool, user_id: Uuid) -> Result<String, AgentError> {
    let plan_opt = crate::db::plans::fetch_active(db, user_id).await?;

    let Some(plan) = plan_opt else {
        return Ok("Geen actief trainingsplan.".into());
    };

    use crate::models::plan::SessionType;

    let total_weeks = plan.weeks.len();
    let total_sessions: usize = plan
        .weeks
        .iter()
        .map(|w| {
            w.days
                .iter()
                .filter(|d| d.session_type != SessionType::Rest)
                .count()
        })
        .sum();
    let completed_sessions: usize = plan
        .weeks
        .iter()
        .map(|w| w.days.iter().filter(|d| d.completed).count())
        .sum();

    let current_week = plan
        .weeks
        .iter()
        .find(|w| {
            w.days
                .iter()
                .any(|d| d.session_type != SessionType::Rest && !d.completed)
        })
        .or_else(|| plan.weeks.last());

    let days_nl = ["Ma", "Di", "Wo", "Do", "Vr", "Za", "Zo"];

    let mut ctx = format!(
        "Plan ID: {}\n{} weken totaal | Voortgang: {}/{} sessies afgerond\n",
        plan.id, total_weeks, completed_sessions, total_sessions
    );

    if let Some(week) = current_week {
        let active_days: Vec<_> = week
            .days
            .iter()
            .filter(|d| d.session_type != SessionType::Rest)
            .collect();
        let week_done = active_days.iter().filter(|d| d.completed).count();

        ctx.push_str(&format!(
            "\nHuidige week: week {} — {} ({})\nTarget: {:.0} km | Afgerond: {}/{} sessies\nSessies:\n",
            week.week_number,
            week.phase.label(),
            if week.is_recovery { "herstelweek" } else { "trainingsweek" },
            week.target_km,
            week_done,
            active_days.len(),
        ));

        for day in &week.days {
            if day.session_type == SessionType::Rest {
                continue;
            }
            let day_name = days_nl.get(day.weekday as usize).unwrap_or(&"?");
            let status = if day.completed { "✓" } else { "·" };
            ctx.push_str(&format!(
                "  {} {}: {} — {:.0} km\n",
                status,
                day_name,
                day.session_type.label(),
                day.effective_km()
            ));
        }
    }

    Ok(ctx)
}

async fn build_injury_context(db: &PgPool, user_id: Uuid) -> Result<String, AgentError> {
    let injuries = crate::db::injuries::fetch_active(db, user_id).await?;

    if injuries.is_empty() {
        return Ok("Geen actieve blessures.".into());
    }

    let mut ctx = format!("{} actieve blessure(s):\n", injuries.len());
    for inj in &injuries {
        let can_run = if inj.can_run {
            "kan hardlopen"
        } else {
            "kan niet hardlopen"
        };
        ctx.push_str(&format!(
            "  - Ernst {}/10, {}, status: {}, gemeld: {}\n",
            inj.severity, can_run, inj.recovery_status, inj.reported_at
        ));
        if let Some(desc) = &inj.description {
            ctx.push_str(&format!("    Omschrijving: {}\n", desc));
        }
    }

    Ok(ctx)
}
