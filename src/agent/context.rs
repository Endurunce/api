use sqlx::PgPool;
use uuid::Uuid;

use crate::{agent::AgentError, db};

/// Build the system prompt for the AI coach, including user profile, plan, and injury data.
pub async fn build_system_prompt(db: &PgPool, user_id: Uuid) -> Result<String, AgentError> {
    let (profile_ctx, plan_opt, injuries, prefs) = tokio::join!(
        db::profiles::fetch_full_by_user(db, user_id),
        db::plans::fetch_active(db, user_id),
        db::injuries::list_active(db, user_id),
        db::training_preferences::fetch_by_user(db, user_id),
    );

    let profile_ctx = profile_ctx
        .map_err(AgentError::Database)?
        .unwrap_or_else(|| "Geen profiel beschikbaar".into());
    let plan_opt = plan_opt.map_err(AgentError::Database)?;
    let injuries = injuries.map_err(AgentError::Database)?;
    let prefs = prefs.map_err(AgentError::Database)?;

    let plan_ctx = if let Some(plan) = &plan_opt {
        let mut ctx = format!(
            "Doel: {}, {} weken\n",
            plan.plan.race_goal,
            plan.weeks.len()
        );
        for week in &plan.weeks {
            ctx.push_str(&format!(
                "Week {}: {} — {:.0} km{}\n",
                week.week.week_number,
                week.week.phase,
                week.week.target_km,
                if week.week.is_recovery { " (herstel)" } else { "" }
            ));
            for session in &week.sessions {
                if session.session_type == "rest" {
                    continue;
                }
                ctx.push_str(&format!(
                    "  dag {}: {} — {:.0} km\n",
                    session.weekday, session.session_type, session.target_km
                ));
            }
        }
        ctx
    } else {
        "Geen actief trainingsplan.".into()
    };

    let injury_ctx = if injuries.is_empty() {
        "Geen actieve blessures.".into()
    } else {
        let mut ctx = format!("{} actieve blessure(s):\n", injuries.len());
        for inj in &injuries {
            ctx.push_str(&format!(
                "  - Ernst {}/10, {}, locaties: {}\n",
                inj.severity,
                if inj.can_run { "kan hardlopen" } else { "kan niet hardlopen" },
                inj.locations.join(", ")
            ));
        }
        ctx
    };

    let prefs_ctx = if let Some(p) = &prefs {
        format!(
            "Trainingsdagen: {:?}, lange loop dag: {}, terrein: {}",
            p.training_days, p.long_run_day, p.terrain
        )
    } else {
        "Geen trainingsvoorkeuren ingesteld.".into()
    };

    let system = format!(
        "Je bent de EnduRunce Coach — persoonlijke AI-hardloopcoach voor duurlopers.\n\
        \n\
        ## Trainingsfilosofie\n\
        80/20-methode: ~80% in Z1-Z2, ~20% in Z3-Z5.\n\
        Blokopbouw: Opbouw I → Opbouw II → Piek → Taper, elke 4e week herstel.\n\
        \n\
        ## Communicatieregels\n\
        - Spreek de gebruiker aan met je/jij.\n\
        - Geef motiverende, concrete adviezen in het Nederlands.\n\
        - Max 3 alinea's tenzij anders gevraagd.\n\
        - Bij blessures: wees voorzichtig.\n\
        \n\
        ## Profiel\n{profile_ctx}\n\n\
        ## Voorkeuren\n{prefs_ctx}\n\n\
        ## Trainingsplan\n{plan_ctx}\n\n\
        ## Blessures\n{injury_ctx}"
    );

    Ok(system)
}
