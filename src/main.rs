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
    let app = app::build_router(state);

    let addr = "0.0.0.0:3000";
    tracing::info!("Endurance API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
