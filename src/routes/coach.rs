use axum::{extract::State, Json};
use serde::Deserialize;

use crate::{
    auth::Claims,
    db,
    errors::{AppError, ApiResult},
    services::anthropic,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
}

/// GET /api/coach — last 60 messages
pub async fn get_messages(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<Vec<db::coach::CoachMessage>>> {
    let messages = db::coach::fetch_messages(&state.db, claims.sub, 60)
        .await
        .map_err(AppError::Database)?;
    Ok(Json(messages))
}

/// POST /api/coach — save user message + stream AI response
pub async fn send_message(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<SendMessageRequest>,
) -> ApiResult<Json<db::coach::CoachMessage>> {
    if req.content.trim().is_empty() {
        return Err(AppError::BadRequest("Bericht mag niet leeg zijn".into()));
    }

    // Save user turn
    db::coach::save_message(&state.db, claims.sub, "user", req.content.trim())
        .await
        .map_err(AppError::Database)?;

    // Build context
    let history = db::coach::fetch_messages(&state.db, claims.sub, 20)
        .await
        .map_err(AppError::Database)?;

    let profile_ctx = db::profiles::fetch_full_by_user(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?
        .unwrap_or_else(|| "Geen profiel beschikbaar".into());

    let plan = db::plans::fetch_active(&state.db, claims.sub)
        .await
        .map_err(AppError::Database)?;

    let plan_ctx = if let Some(p) = plan {
        let done = p.weeks.iter().filter(|w| w.days.iter().any(|d| d.completed)).count();
        format!("{} weken plan, {} weken gestart.", p.weeks.len(), done)
    } else {
        "Geen actief trainingsplan.".into()
    };

    let system = format!(
        "Je bent de EnduRunce Coach — persoonlijke AI-hardloopcoach. \
        Geef motiverende, concrete adviezen in het Nederlands. \
        Spreek de gebruiker aan met je/jij. Max 3 alinea's tenzij anders gevraagd.\n\
        Profiel: {}\nPlan: {}",
        profile_ctx, plan_ctx
    );

    let messages: Vec<anthropic::Message> = history
        .iter()
        .map(|m| anthropic::Message {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect();

    let ai_text = anthropic::complete(Some(&system), messages, 1024)
        .await
        .unwrap_or_else(|_| {
            "Er is een fout opgetreden. Probeer het opnieuw. 🔄".into()
        });

    // Save assistant turn
    let assistant_msg = db::coach::save_message(&state.db, claims.sub, "assistant", &ai_text)
        .await
        .map_err(AppError::Database)?;

    // Prune old messages (keep 60)
    db::coach::prune_old_messages(&state.db, claims.sub, 60).await.ok();

    Ok(Json(assistant_msg))
}
