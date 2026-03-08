use sqlx::PgPool;
use uuid::Uuid;

pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
}

pub async fn insert(db: &PgPool, email: &str, password_hash: &str) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        INSERT INTO users (email, password_hash)
        VALUES ($1, $2)
        RETURNING id
        "#,
        email,
        password_hash,
    )
    .fetch_one(db)
    .await?;

    Ok(row.id)
}

pub async fn fetch_by_email(db: &PgPool, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT id, email, password_hash FROM users WHERE email = $1",
        email,
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| UserRow {
        id: r.id,
        email: r.email,
        password_hash: r.password_hash,
    }))
}

pub async fn exists(db: &PgPool, email: &str) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!("SELECT 1 AS one FROM users WHERE email = $1", email)
        .fetch_optional(db)
        .await?;
    Ok(row.is_some())
}
