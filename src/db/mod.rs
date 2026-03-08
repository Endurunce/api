pub mod coach;
pub mod feedback;
pub mod injuries;
pub mod plans;
pub mod profiles;
pub mod strava;
pub mod users;

use sqlx::PgPool;

pub type Db = PgPool;

pub async fn connect(database_url: &str) -> Result<Db, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}
