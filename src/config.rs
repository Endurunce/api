/// Centralized configuration loaded once at startup.
/// Panics if required env vars are missing.
#[derive(Clone)]
pub struct Config {
    pub jwt_secret: String,
    pub database_url: String,
    pub strava_client_id: Option<String>,
    pub strava_client_secret: Option<String>,
    pub strava_redirect_uri: Option<String>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub google_redirect_uri: Option<String>,
    pub app_url: String,
    pub admin_url: String,
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,
    pub allowed_origins: Vec<String>,
}

impl Config {
    /// Load configuration from environment variables.
    /// Panics if `JWT_SECRET` or `DATABASE_URL` are not set.
    pub fn from_env() -> Self {
        let jwt_secret = std::env::var("JWT_SECRET")
            .expect("JWT_SECRET must be set — refusing to start with a default secret");
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set");

        let allowed_origins = std::env::var("ALLOWED_ORIGINS")
            .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|_| vec![
                "https://app.endurunce.nl".into(),
                "https://admin.endurunce.nl".into(),
                "http://localhost:3000".into(),
                "http://localhost:8080".into(),
            ]);

        Config {
            jwt_secret,
            database_url,
            strava_client_id: std::env::var("STRAVA_CLIENT_ID").ok(),
            strava_client_secret: std::env::var("STRAVA_CLIENT_SECRET").ok(),
            strava_redirect_uri: std::env::var("STRAVA_REDIRECT_URI").ok(),
            google_client_id: std::env::var("GOOGLE_CLIENT_ID").ok(),
            google_client_secret: std::env::var("GOOGLE_CLIENT_SECRET").ok(),
            google_redirect_uri: std::env::var("GOOGLE_REDIRECT_URI").ok(),
            app_url: std::env::var("APP_URL")
                .unwrap_or_else(|_| "https://app.endurunce.nl".into()),
            admin_url: std::env::var("ADMIN_URL")
                .unwrap_or_else(|_| "https://admin.endurunce.nl".into()),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            anthropic_model: std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-6".into()),
            allowed_origins,
        }
    }
}
