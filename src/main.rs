mod auth;
mod db;
mod errors;
mod models;
mod routes;
mod services;

use axum::{
    routing::{get, patch, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
pub struct AppState {
    pub db: db::Db,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "endurance=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = db::connect(&database_url).await.expect("Failed to connect to database");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run migrations");

    let state = AppState { db };

    let app = Router::new()
        // Health
        .route("/health", get(routes::health::health_check))
        // Auth — email/password
        .route("/api/auth/register", post(routes::auth::register))
        .route("/api/auth/login",    post(routes::auth::login))
        // Auth — Strava OAuth (login/register)
        .route("/api/auth/strava",            get(routes::strava::auth_url))
        .route("/api/strava/callback",        get(routes::strava::callback))
        // Auth — Google OAuth
        .route("/api/auth/google",            get(routes::google::auth_url))
        .route("/api/auth/google/callback",   get(routes::google::callback))
        // Plans (protected)
        .route("/api/plans/generate", post(routes::plans::generate))
        .route("/api/plans",          get(routes::plans::get_active))
        .route("/api/plans/:plan_id", get(routes::plans::get_by_id))
        // Session feedback (protected)
        .route(
            "/api/plans/:plan_id/weeks/:week/days/:weekday/complete",
            post(routes::feedback::complete_day),
        )
        // Session detail advice (protected)
        .route(
            "/api/plans/:plan_id/weeks/:week/days/:weekday/advice",
            get(routes::sessions::session_advice),
        )
        // Uncomplete a session
        .route(
            "/api/plans/:plan_id/weeks/:week/days/:weekday/uncomplete",
            post(routes::sessions::uncomplete_day),
        )
        // Injuries (protected)
        .route("/api/injuries",
            post(routes::injuries::report_injury)
            .get(routes::injuries::list_injuries),
        )
        .route("/api/injuries/:id/resolve", patch(routes::injuries::resolve_injury))
        // Strava — link existing account (protected)
        .route("/api/strava/connect",         get(routes::strava::connect))
        .route("/api/strava/exchange-code",   post(routes::strava::exchange_code))
        .route("/api/strava/status",          get(routes::strava::status))
        .route("/api/strava/activities",      get(routes::strava::activities))
        // Profile (protected)
        .route("/api/profiles/me", get(routes::profiles::me).patch(routes::profiles::update_me))
        // Coach (protected)
        .route("/api/coach",
            get(routes::coach::get_messages)
            .post(routes::coach::send_message),
        )
        // Admin (protected + is_admin)
        .route("/api/admin/stats",             get(routes::admin::stats))
        .route("/api/admin/users",             get(routes::admin::list_users))
        .route("/api/admin/users/:id/admin",   patch(routes::admin::set_admin))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr = "0.0.0.0:3000";
    tracing::info!("Endurance API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
