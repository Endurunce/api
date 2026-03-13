pub mod activities;
pub mod injuries;
pub mod oauth_sessions;
pub mod plans;
pub mod profiles;
pub mod strava;
pub mod training_preferences;
pub mod users;

pub type Db = sqlx::PgPool;

pub async fn connect(url: &str) -> Result<Db, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await
}
