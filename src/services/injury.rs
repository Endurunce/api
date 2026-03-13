use sqlx::PgPool;

use crate::models::injury::{Injury, SeverityClass};
use crate::models::plan::FullPlan;
use crate::db;

/// Adapt a plan for an injury by reducing/resting affected sessions.
/// Returns true if any changes were made.
pub async fn adapt_plan_for_injury(
    pool: &PgPool,
    plan: &FullPlan,
    injury: &Injury,
) -> Result<bool, sqlx::Error> {
    let severity = injury.severity_class();
    let mut changes_made = false;

    for week in &plan.weeks {
        for session in &week.sessions {
            let should_modify = match &severity {
                SeverityClass::Severe => {
                    // Severe: rest all running sessions
                    session.session_type != "rest" && session.session_type != "strength"
                }
                SeverityClass::Moderate => {
                    // Moderate: reduce intensity (convert tempo/interval to easy, halve km)
                    session.session_type == "tempo"
                        || session.session_type == "interval"
                        || session.session_type == "long"
                }
                SeverityClass::Mild => {
                    // Mild: only reduce interval sessions
                    session.session_type == "interval"
                }
            };

            if should_modify {
                let (new_type, new_km) = match &severity {
                    SeverityClass::Severe => ("rest", 0.0),
                    SeverityClass::Moderate => {
                        if session.session_type == "long" {
                            ("easy", session.target_km * 0.5)
                        } else {
                            ("easy", session.target_km * 0.7)
                        }
                    }
                    SeverityClass::Mild => {
                        ("easy", session.target_km * 0.8)
                    }
                };

                let note = format!(
                    "Aangepast wegens blessure (ernst {}/10): {} → {}",
                    injury.severity, session.session_type, new_type
                );

                db::plans::update_session(
                    pool,
                    session.id,
                    session.user_id,
                    Some(new_type),
                    Some(new_km),
                    Some(&note),
                )
                .await?;

                changes_made = true;
            }
        }
    }

    Ok(changes_made)
}
