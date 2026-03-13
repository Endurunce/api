use axum::{
    extract::{ConnectInfo, DefaultBodyLimit},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Router,
};
use http::header;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{agent, routes, AppState};

// ── Per-IP rate limiter for auth endpoints ────────────────────────────────────

/// Maximum number of auth attempts per IP per window.
const AUTH_RATE_LIMIT: u32 = 5;
/// Rate limit window duration in seconds (1 minute).
const AUTH_RATE_WINDOW_SECS: u64 = 60;

/// Shared state for auth rate limiting: maps IP → (attempt count, window start).
type RateLimitState = Arc<Mutex<HashMap<IpAddr, (u32, std::time::Instant)>>>;

/// Middleware that enforces per-IP rate limiting on auth endpoints.
/// Allows [`AUTH_RATE_LIMIT`] requests per [`AUTH_RATE_WINDOW_SECS`] per IP.
async fn auth_rate_limit(
    rate_state: RateLimitState,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Extract client IP from X-Forwarded-For (reverse proxy) or peer address
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip())
        })
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));

    let now = std::time::Instant::now();
    {
        let mut map = rate_state.lock().await;
        let entry = map.entry(ip).or_insert((0, now));

        // Reset window if expired
        if now.duration_since(entry.1).as_secs() >= AUTH_RATE_WINDOW_SECS {
            *entry = (0, now);
        }

        entry.0 += 1;
        if entry.0 > AUTH_RATE_LIMIT {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(serde_json::json!({
                    "error": "Too many attempts. Try again in a minute."
                })),
            )
                .into_response();
        }
    }

    next.run(req).await
}

/// Builds the full Axum router. Called from main() and from integration tests.
pub fn build_router(state: AppState) -> Router {
    // Build CORS layer from config
    let cors = {
        let origins: Vec<http::HeaderValue> = state
            .config
            .allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                http::Method::GET,
                http::Method::POST,
                http::Method::PATCH,
                http::Method::DELETE,
                http::Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
            ])
    };

    // Auth rate limiter shared state
    let rate_limit: RateLimitState = Arc::new(Mutex::new(HashMap::new()));
    let rl = rate_limit.clone();

    // Auth routes with per-IP rate limiting
    let auth_routes = Router::new()
        .route("/api/auth/register", post(routes::auth::register))
        .route("/api/auth/login",    post(routes::auth::login))
        .layer(middleware::from_fn(move |req, next| {
            let rl = rl.clone();
            auth_rate_limit(rl, req, next)
        }))
        .with_state(state.clone());

    let mut router = Router::new()
        // Health
        .route("/health", get(routes::health::health_check))
        // Auth — email/password (rate limited)
        .merge(auth_routes)
        // Auth — Strava OAuth
        .route("/api/auth/strava",          get(routes::strava::auth_url))
        .route("/api/strava/callback",      get(routes::strava::callback))
        // Auth — Google OAuth
        .route("/api/auth/google",          get(routes::google::auth_url))
        .route("/api/auth/google/callback", get(routes::google::callback))
        // Auth — OAuth session exchange
        .route("/api/auth/session/:id",     get(routes::oauth_session::get_session))
        // Plans (protected)
        .route("/api/plans/generate", post(routes::plans::generate))
        .route("/api/plans",          get(routes::plans::get_active))
        .route("/api/plans/:plan_id", get(routes::plans::get_by_id))
        // Session feedback (protected)
        .route(
            "/api/plans/:plan_id/weeks/:week/days/:weekday/complete",
            post(routes::feedback::complete_day),
        )
        .route(
            "/api/plans/:plan_id/weeks/:week/days/:weekday/advice",
            get(routes::sessions::session_advice),
        )
        .route(
            "/api/plans/:plan_id/weeks/:week/days/:weekday/uncomplete",
            post(routes::sessions::uncomplete_day),
        )
        // Injuries (protected)
        .route(
            "/api/injuries",
            post(routes::injuries::report_injury).get(routes::injuries::list_injuries),
        )
        .route("/api/injuries/history", get(routes::injuries::injury_history))
        .route("/api/injuries/:id/resolve", patch(routes::injuries::resolve_injury))
        // Strava (protected)
        .route("/api/strava/connect",       get(routes::strava::connect))
        .route("/api/strava/exchange-code", post(routes::strava::exchange_code))
        .route("/api/strava/status",        get(routes::strava::status))
        .route("/api/strava/activities",    get(routes::strava::activities))
        // Profile (protected)
        .route("/api/profiles/me", get(routes::profiles::me).patch(routes::profiles::update_me))
        // Coach (protected) — legacy REST endpoint
        .route(
            "/api/coach",
            get(routes::coach::get_messages).post(routes::coach::send_message),
        )
        // Conversation history (protected)
        .route("/api/conversations", get(routes::conversations::list))
        // Intake — conversational onboarding (REST)
        .route("/api/intake/start", post(routes::intake::start))
        .route("/api/intake/reply", post(routes::intake::reply))
        // AI Coach Agent — WebSocket streaming
        .route("/api/ws", get(agent::streaming::ws_handler))
        // Admin (protected + is_admin)
        .route("/api/admin/stats",             get(routes::admin::stats))
        .route("/api/admin/users",             get(routes::admin::list_users))
        .route("/api/admin/users/:id/admin",   patch(routes::admin::set_admin));

    // Test helpers — only registered when TEST_MODE=true (checked at router build time)
    if std::env::var("TEST_MODE").ok().as_deref() == Some("true") {
        router = router.route(
            "/api/test/oauth-session",
            post(routes::test_helpers::create_oauth_session),
        );
    }

    router
        .with_state(state)
        .layer(DefaultBodyLimit::max(1_048_576)) // 1 MB
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

// ── Integration tests ─────────────────────────────────────────────────────────
//
// Vereisten om te draaien:
//   DATABASE_URL=postgres://user:pass@localhost/postgres cargo test
//
// sqlx::test maakt per test een verse database aan (met migraties) en ruimt
// hem daarna op. De tests raken de productiedatabase nooit.

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use tower::ServiceExt; // oneshot

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn app(pool: PgPool) -> Router {
        let config = crate::config::Config {
            jwt_secret: "test_secret".into(),
            database_url: String::new(),
            strava_client_id: None,
            strava_client_secret: None,
            strava_redirect_uri: None,
            google_client_id: None,
            google_client_secret: None,
            google_redirect_uri: None,
            app_url: "http://localhost:8080".into(),
            admin_url: "http://localhost:8081".into(),
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-6".into(),
            allowed_origins: vec!["http://localhost:8080".into()],
        };
        build_router(AppState { db: pool, config, http: reqwest::Client::new() })
    }

    /// POST/PATCH/etc. with a JSON body.
    fn json_req(method: Method, uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    /// Any method with a JSON body + Bearer token.
    fn authed_json(method: Method, uri: &str, body: Value, token: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    /// GET with a Bearer token.
    fn authed_get(uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap()
    }

    /// PATCH with no body + Bearer token.
    fn authed_patch(uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method(Method::PATCH)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap()
    }

    /// Collect the response body as parsed JSON.
    async fn json_body(resp: axum::response::Response) -> Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    /// Register a new user; returns (status, body).
    async fn do_register(app: &Router, email: &str, pass: &str) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(json_req(
                Method::POST,
                "/api/auth/register",
                json!({"email": email, "password": pass}),
            ))
            .await
            .unwrap();
        let status = resp.status();
        (status, json_body(resp).await)
    }

    /// Register a user and return their JWT token (panics if registration fails).
    async fn register_and_token(app: &Router, email: &str, pass: &str) -> String {
        let (status, body) = do_register(app, email, pass).await;
        assert_eq!(status, StatusCode::CREATED, "registration failed: {body}");
        body["token"].as_str().unwrap().to_string()
    }

    /// Minimal valid Profile fixture (age ~36, marathon, race date in future).
    fn profile() -> Value {
        json!({
            "id":                  "00000000-0000-0000-0000-000000000001",
            "user_id":             "00000000-0000-0000-0000-000000000001",
            "name":                "Test Runner",
            "date_of_birth":       "1990-06-15",
            "gender":              "male",
            "running_years":       "two_to_five_years",
            "weekly_km":           40.0,
            "previous_ultra":      "none",
            "time_10k":            null,
            "time_half_marathon":  null,
            "time_marathon":       null,
            "race_goal":           "marathon",
            "race_time_goal":      null,
            "race_date":           "2027-04-15",
            "terrain":             "road",
            "training_days":       [1, 3, 5, 6],
            "strength_days":       [],
            "max_duration_per_day": [],
            "long_run_day":        6,
            "max_hr":              null,
            "rest_hr":             55,
            "hr_zones":            null,
            "sleep_hours":         "seven_to_eight",
            "complaints":          null,
            "previous_injuries":   []
        })
    }

    /// Same fixture but with a date_of_birth that makes the user 11 years old.
    fn underage_profile() -> Value {
        let mut p = profile();
        p["date_of_birth"] = json!("2015-01-01");
        p
    }

    // ── Health ────────────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn health_check_returns_200(pool: PgPool) {
        let resp = app(pool)
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["db"], "connected");
    }

    // ── Auth — register ───────────────────────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn register_returns_201_with_token(pool: PgPool) {
        let app = app(pool);
        let (status, body) = do_register(&app, "runner@example.com", "password123").await;

        assert_eq!(status, StatusCode::CREATED);
        assert!(body["token"].is_string(), "response must contain a token");
        assert!(body["user_id"].is_string());
        assert_eq!(body["email"], "runner@example.com");
        assert_eq!(body["is_admin"], false);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn register_duplicate_email_returns_400(pool: PgPool) {
        let app = app(pool);
        do_register(&app, "dup@example.com", "password123").await;
        let (status, body) = do_register(&app, "dup@example.com", "other_password").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body["error"].as_str().unwrap().to_lowercase().contains("already"),
            "error message should mention duplicate: {body}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn register_short_password_returns_400(pool: PgPool) {
        let app = app(pool);
        let (status, body) = do_register(&app, "new@example.com", "short").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body["error"].as_str().unwrap().to_lowercase().contains("password"),
            "error should mention password: {body}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn register_invalid_email_returns_400(pool: PgPool) {
        let app = app(pool);
        let (status, _) = do_register(&app, "not-an-email", "password123").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    // ── Auth — login ──────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn login_correct_credentials_returns_200(pool: PgPool) {
        let app = app(pool);
        do_register(&app, "login@example.com", "mypassword").await;

        let resp = app
            .clone()
            .oneshot(json_req(
                Method::POST,
                "/api/auth/login",
                json!({"email": "login@example.com", "password": "mypassword"}),
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert!(body["token"].is_string(), "login must return a token");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn login_wrong_password_returns_400(pool: PgPool) {
        let app = app(pool);
        do_register(&app, "user@example.com", "correctpassword").await;

        let resp = app
            .oneshot(json_req(
                Method::POST,
                "/api/auth/login",
                json!({"email": "user@example.com", "password": "wrongpassword"}),
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn login_unknown_email_returns_400(pool: PgPool) {
        let app = app(pool);
        let resp = app
            .oneshot(json_req(
                Method::POST,
                "/api/auth/login",
                json!({"email": "nobody@example.com", "password": "password123"}),
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── Protected routes require auth ─────────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn protected_routes_return_401_without_token(pool: PgPool) {
        let app = app(pool);

        let cases: &[(Method, &str)] = &[
            (Method::GET,  "/api/plans"),
            (Method::POST, "/api/plans/generate"),
            (Method::GET,  "/api/injuries"),
            (Method::POST, "/api/injuries"),
            (Method::GET,  "/api/profiles/me"),
            (Method::GET,  "/api/coach"),
            (Method::POST, "/api/coach"),
            (Method::GET,  "/api/admin/stats"),
        ];

        for (method, uri) in cases {
            let req = Request::builder()
                .method(method.clone())
                .uri(*uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap();

            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::UNAUTHORIZED,
                "{} {} should return 401 without auth", method, uri
            );
        }
    }

    // ── Plans ─────────────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn get_plan_returns_404_before_generate(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        let resp = app.clone().oneshot(authed_get("/api/plans", &tok)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn generate_plan_returns_201(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        let resp = app
            .clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/plans/generate",
                json!({"profile": profile()}),
                &tok,
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = json_body(resp).await;
        assert!(body["plan_id"].is_string());
        assert!(body["num_weeks"].as_u64().unwrap() > 0);
        assert!(!body["plan"]["weeks"].as_array().unwrap().is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_active_plan_after_generate(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        app.clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/plans/generate",
                json!({"profile": profile()}),
                &tok,
            ))
            .await
            .unwrap();

        let resp = app.clone().oneshot(authed_get("/api/plans", &tok)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert!(!body["weeks"].as_array().unwrap().is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn generate_replaces_existing_plan(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        // Generate twice — second should deactivate the first
        for _ in 0..2 {
            let resp = app
                .clone()
                .oneshot(authed_json(
                    Method::POST,
                    "/api/plans/generate",
                    json!({"profile": profile()}),
                    &tok,
                ))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::CREATED);
        }

        // Should still return exactly one active plan
        let resp = app.clone().oneshot(authed_get("/api/plans", &tok)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn generate_plan_underage_returns_400(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "young@example.com", "password123").await;

        let resp = app
            .oneshot(authed_json(
                Method::POST,
                "/api/plans/generate",
                json!({"profile": underage_profile()}),
                &tok,
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = json_body(resp).await;
        assert!(
            body["error"].as_str().unwrap().contains("16"),
            "error should mention minimum age: {body}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_plan_by_id_scoped_to_owner(pool: PgPool) {
        let app = app(pool);
        let tok1 = register_and_token(&app, "user1@example.com", "password123").await;
        let tok2 = register_and_token(&app, "user2@example.com", "password123").await;

        // User 1 generates a plan
        let resp = app
            .clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/plans/generate",
                json!({"profile": profile()}),
                &tok1,
            ))
            .await
            .unwrap();
        let plan_id = json_body(resp).await["plan_id"].as_str().unwrap().to_string();

        // User 2 should not be able to fetch user 1's plan by ID
        let resp = app
            .clone()
            .oneshot(authed_get(&format!("/api/plans/{}", plan_id), &tok2))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── Injuries ──────────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn report_injury_returns_201(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        let resp = app
            .clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/injuries",
                json!({
                    "locations":   ["knee"],
                    "severity":    5,
                    "can_walk":    true,
                    "can_run":     false,
                    "description": "Lichte kniepijn"
                }),
                &tok,
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = json_body(resp).await;
        assert!(body["injury_id"].is_string());
        assert_eq!(body["plan_adapted"], false); // no plan active
        assert!(body["recovery_weeks"].as_u64().unwrap() > 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn report_injury_adapts_active_plan(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        // Generate a plan first
        app.clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/plans/generate",
                json!({"profile": profile()}),
                &tok,
            ))
            .await
            .unwrap();

        // Report a moderate injury
        let resp = app
            .clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/injuries",
                json!({"locations": ["knee"], "severity": 5, "can_walk": true, "can_run": false}),
                &tok,
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = json_body(resp).await;
        assert_eq!(body["plan_adapted"], true, "plan should be adapted when one is active");
        assert!(body["recovery_weeks"].as_u64().unwrap() > 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn injury_severity_out_of_range_returns_400(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        for bad in [0u8, 11u8] {
            let resp = app
                .clone()
                .oneshot(authed_json(
                    Method::POST,
                    "/api/injuries",
                    json!({"locations": ["knee"], "severity": bad, "can_walk": true, "can_run": false}),
                    &tok,
                ))
                .await
                .unwrap();
            assert_eq!(
                resp.status(), StatusCode::BAD_REQUEST,
                "severity {} should be rejected", bad
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_injuries_returns_reported_injury(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        // Initially empty
        let resp = app.clone().oneshot(authed_get("/api/injuries", &tok)).await.unwrap();
        assert_eq!(json_body(resp).await.as_array().unwrap().len(), 0);

        // Report one
        app.clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/injuries",
                json!({"locations": ["achilles"], "severity": 3, "can_walk": true, "can_run": true}),
                &tok,
            ))
            .await
            .unwrap();

        let resp = app.clone().oneshot(authed_get("/api/injuries", &tok)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["severity"], 3);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn resolve_injury_removes_from_active_list(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        // Report
        let resp = app
            .clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/injuries",
                json!({"locations": ["shin"], "severity": 2, "can_walk": true, "can_run": true}),
                &tok,
            ))
            .await
            .unwrap();
        let injury_id = json_body(resp).await["injury_id"].as_str().unwrap().to_string();

        // Resolve
        let resp = app
            .clone()
            .oneshot(authed_patch(&format!("/api/injuries/{}/resolve", injury_id), &tok))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Should no longer appear in active list
        let resp = app.clone().oneshot(authed_get("/api/injuries", &tok)).await.unwrap();
        assert_eq!(json_body(resp).await.as_array().unwrap().len(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn resolve_unknown_injury_returns_404(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        let resp = app
            .oneshot(authed_patch(
                "/api/injuries/00000000-0000-0000-0000-000000000099/resolve",
                &tok,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn injuries_are_scoped_to_user(pool: PgPool) {
        let app = app(pool);
        let tok1 = register_and_token(&app, "user1@example.com", "password123").await;
        let tok2 = register_and_token(&app, "user2@example.com", "password123").await;

        // User 1 reports an injury
        app.clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/injuries",
                json!({"locations": ["hip"], "severity": 4, "can_walk": true, "can_run": false}),
                &tok1,
            ))
            .await
            .unwrap();

        // User 2 should see an empty list
        let resp = app.clone().oneshot(authed_get("/api/injuries", &tok2)).await.unwrap();
        assert_eq!(
            json_body(resp).await.as_array().unwrap().len(), 0,
            "user2 should not see user1's injuries"
        );
    }

    // ── Profiles ──────────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn get_profile_returns_null_before_plan_generation(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        let resp = app.clone().oneshot(authed_get("/api/profiles/me", &tok)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(json_body(resp).await.is_null());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_profile_returns_data_after_plan_generation(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        app.clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/plans/generate",
                json!({"profile": profile()}),
                &tok,
            ))
            .await
            .unwrap();

        let resp = app.clone().oneshot(authed_get("/api/profiles/me", &tok)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert_eq!(body["name"], "Test Runner");
        assert_eq!(body["gender"], "male");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn update_profile_name_persists(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "runner@example.com", "password123").await;

        // First generate a plan (creates the profile row)
        app.clone()
            .oneshot(authed_json(
                Method::POST,
                "/api/plans/generate",
                json!({"profile": profile()}),
                &tok,
            ))
            .await
            .unwrap();

        // Update name
        let resp = app
            .clone()
            .oneshot(authed_json(
                Method::PATCH,
                "/api/profiles/me",
                json!({"name": "Renamed Runner"}),
                &tok,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Verify
        let resp = app.clone().oneshot(authed_get("/api/profiles/me", &tok)).await.unwrap();
        assert_eq!(json_body(resp).await["name"], "Renamed Runner");
    }

    // ── Admin ─────────────────────────────────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn admin_stats_as_regular_user_returns_403(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "user@example.com", "password123").await;

        let resp = app.oneshot(authed_get("/api/admin/stats", &tok)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn admin_users_list_as_regular_user_returns_403(pool: PgPool) {
        let app = app(pool);
        let tok = register_and_token(&app, "user@example.com", "password123").await;

        let resp = app.oneshot(authed_get("/api/admin/users", &tok)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
