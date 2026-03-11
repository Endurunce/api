mod app;
mod auth;
mod db;
mod errors;
mod models;
mod routes;
mod services;

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
    let db = {
        let mut last_err = None;
        let mut db = None;
        for attempt in 1..=6 {
            match db::connect(&database_url).await {
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

    let state = AppState { db };
    let app = app::build_router(state);

    let addr = "0.0.0.0:3000";
    tracing::info!("Endurance API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
