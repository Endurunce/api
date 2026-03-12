# Endurunce API

Personalized AI-powered running coach backend built with **Rust**, **Axum**, and **PostgreSQL**.

Generates periodized training plans, adapts them for injuries, and provides an AI coach (Claude) that can read and modify the user's plan in real-time via tool use.

## Prerequisites

- **Rust** ≥ 1.75 (with `cargo`)
- **PostgreSQL** ≥ 15
- **sqlx-cli** — `cargo install sqlx-cli --no-default-features --features rustls,postgres`

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | ✅ | — | PostgreSQL connection string |
| `JWT_SECRET` | ✅ | — | HMAC secret for JWT signing (panics if missing) |
| `RUST_LOG` | | `endurance=debug,tower_http=debug` | Tracing log filter |
| `APP_URL` | | `https://app.endurunce.nl` | Flutter web app base URL |
| `ADMIN_URL` | | `https://admin.endurunce.nl` | Admin panel base URL |
| `ALLOWED_ORIGINS` | | `https://app.endurunce.nl,https://admin.endurunce.nl,http://localhost:3000,http://localhost:8080` | Comma-separated CORS origins |
| `STRAVA_CLIENT_ID` | | — | Server-level Strava OAuth client ID |
| `STRAVA_CLIENT_SECRET` | | — | Server-level Strava OAuth client secret |
| `STRAVA_REDIRECT_URI` | | — | Strava OAuth redirect URI |
| `GOOGLE_CLIENT_ID` | | — | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | | — | Google OAuth client secret |
| `GOOGLE_REDIRECT_URI` | | — | Google OAuth redirect URI |
| `ANTHROPIC_API_KEY` | | — | Anthropic API key for AI coach |
| `ANTHROPIC_MODEL` | | `claude-sonnet-4-6` | Claude model to use |
| `TEST_MODE` | | — | Set to `true` to enable test helper endpoints |

## Local Development

```bash
# Clone and enter the project
git clone https://github.com/Endurunce/api.git
cd api

# Copy and edit environment variables
cp .env.example .env
# Edit .env with your DATABASE_URL and JWT_SECRET

# Run database migrations
sqlx migrate run

# Generate offline query metadata (for CI builds without a live DB)
cargo sqlx prepare

# Run the server
cargo run
# → listening on 0.0.0.0:3000
```

## Migrations

```bash
# Run all pending migrations
sqlx migrate run

# Add a new migration
sqlx migrate add <name>

# Revert the last migration
sqlx migrate revert
```

## Tests

Integration tests use `sqlx::test` which creates a temporary database per test (requires `DATABASE_URL`):

```bash
DATABASE_URL=postgres://user:pass@localhost/endurunce_test cargo test
```

For CI without a live database, use offline mode:

```bash
SQLX_OFFLINE=true cargo test
```

## API Endpoints

### Public
| Method | Path | Description |
|---|---|---|
| GET | `/health` | Health check |
| POST | `/api/auth/register` | Register (email + password) |
| POST | `/api/auth/login` | Login (email + password) |
| GET | `/api/auth/strava` | Strava OAuth URL |
| GET | `/api/strava/callback` | Strava OAuth callback |
| GET | `/api/auth/google` | Google OAuth URL |
| GET | `/api/auth/google/callback` | Google OAuth callback |
| GET | `/api/auth/session/:id` | Exchange OAuth session for JWT |

### Protected (Bearer JWT)
| Method | Path | Description |
|---|---|---|
| POST | `/api/plans/generate` | Generate a training plan |
| GET | `/api/plans` | Get active plan |
| GET | `/api/plans/:id` | Get plan by ID |
| POST | `/api/plans/:id/weeks/:w/days/:d/complete` | Complete a session |
| GET | `/api/plans/:id/weeks/:w/days/:d/advice` | AI session advice |
| POST | `/api/plans/:id/weeks/:w/days/:d/uncomplete` | Undo completion |
| POST | `/api/injuries` | Report injury |
| GET | `/api/injuries` | List active injuries |
| GET | `/api/injuries/history` | Full injury history |
| PATCH | `/api/injuries/:id/resolve` | Resolve injury |
| GET | `/api/strava/connect` | Link Strava account |
| POST | `/api/strava/exchange-code` | Exchange code (user credentials) |
| GET | `/api/strava/status` | Strava connection status |
| GET | `/api/strava/activities` | Fetch Strava activities |
| GET | `/api/profiles/me` | Get profile |
| PATCH | `/api/profiles/me` | Update profile |
| GET | `/api/coach` | Get coach messages |
| POST | `/api/coach` | Send coach message |
| GET | `/api/ws` | WebSocket AI coach agent |

### Admin (Bearer JWT + is_admin)
| Method | Path | Description |
|---|---|---|
| GET | `/api/admin/stats` | Platform statistics |
| GET | `/api/admin/users` | Paginated user list |
| PATCH | `/api/admin/users/:id/admin` | Set admin status |

## Deployment (Fly.io)

The app is deployed to [Fly.io](https://fly.io) via the CD workflow:

1. Push to `master` triggers `.github/workflows/cd.yml`
2. Docker image is built and pushed to `ghcr.io/endurunce/api`
3. Fly.io deploys the pre-built image

Manual deployment:

```bash
flyctl deploy --image ghcr.io/endurunce/api:latest
```

## Architecture

```
src/
├── agent/          # AI coach agent (tool use, streaming, memory)
├── config.rs       # Centralized env var config
├── db/             # Database queries (sqlx)
├── errors.rs       # Error types → HTTP status mapping
├── models/         # Domain models (Plan, Profile, Injury, Feedback)
├── routes/         # Axum route handlers
├── services/       # Business logic (schedule generation, Anthropic API)
├── app.rs          # Router construction
├── auth.rs         # JWT encoding/decoding, Claims extractor
└── main.rs         # Entry point, DB connection, graceful shutdown
```

## License

Proprietary — © Endurunce
