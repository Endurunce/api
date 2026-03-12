mod agent;
mod app;
mod auth;
mod config;
mod db;
mod errors;
mod models;
mod routes;
mod services;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: db::Db,
    pub config: Config,
    pub http: reqwest::Client,
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

    let config = Config::from_env();

    let db = {
        let mut last_err = None;
        let mut db = None;
        for attempt in 1..=6 {
            match db::connect(&config.database_url).await {
                Ok(pool) => { db = Some(pool); break; }
                Err(e) => {
                    tracing::warn!("Database connection attempt {}/6 failed: {}", attempt, e);
                    last_err = Some(e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                }
            }
        }
        db.unwrap_or_else(|| panic!("Failed to connect to database after 6 attempts: {:?}", last_err))
    };

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run migrations");

    let http = reqwest::Client::new();
    let state = AppState { db, config, http };
    let app = app::build_router(state.clone());

    // Background task: clean up expired OAuth sessions every hour
    spawn_oauth_session_cleanup(state.db.clone());

    let addr = "0.0.0.0:3000";
    tracing::info!("Endurance API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

/// Spawn a background Tokio task that deletes expired OAuth sessions every hour.
fn spawn_oauth_session_cleanup(db: db::Db) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match sqlx::query("DELETE FROM oauth_sessions WHERE created_at < NOW() - INTERVAL '1 hour'")
                .execute(&db)
                .await
            {
                Ok(result) => {
                    let count = result.rows_affected();
                    if count > 0 {
                        tracing::info!("Cleaned up {} expired OAuth sessions", count);
                    }
                }
                Err(e) => {
                    tracing::warn!("OAuth session cleanup failed: {}", e);
                }
            }
        }
    });
}

/// Listen for SIGTERM (Fly.io / container orchestrators) to trigger graceful shutdown.
/// In-flight requests will be allowed to complete before the server stops.
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate())
        .expect("failed to register SIGTERM handler");

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, starting graceful shutdown");
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C, starting graceful shutdown");
        }
    }
}
