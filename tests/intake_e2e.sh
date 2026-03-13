#!/bin/bash
# E2E test for the intake flow via REST endpoints
# Usage: ./tests/intake_e2e.sh [API_URL]
#
# Tests the full intake conversation flow:
# 1. Register a test user
# 2. Start intake
# 3. Step through all intake questions
# 4. Verify plan generation
# 5. Clean up test user

set -euo pipefail

API_URL="${1:-https://api.endurunce.nl}"
EMAIL="intake-test-$(date +%s)@test.endurunce.nl"
PASS="TestPass123!"
TOKEN=""
ERRORS=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

pass() { echo -e "${GREEN}✓${NC} $1"; }
fail() { echo -e "${RED}✗${NC} $1"; ERRORS=$((ERRORS + 1)); }
info() { echo -e "${YELLOW}→${NC} $1"; }

# ── Register ────────────────────────────────────────────────────────────────

info "Registering test user: $EMAIL"
RESP=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/auth/register" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$EMAIL\",\"password\":\"$PASS\"}")
CODE=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | head -1)

if [ "$CODE" = "200" ] || [ "$CODE" = "201" ]; then
  TOKEN=$(echo "$BODY" | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
  if [ -n "$TOKEN" ]; then
    pass "Registered (token: ${TOKEN:0:20}...)"
  else
    fail "Registered but no token in response: $BODY"
  fi
else
  fail "Registration failed ($CODE): $BODY"
  exit 1
fi

# ── Helper: intake reply ────────────────────────────────────────────────────

intake_reply() {
  local value="$1"
  local expected_step="${2:-}"
  
  RESP=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/intake/reply" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d "{\"value\":\"$value\"}")
  CODE=$(echo "$RESP" | tail -1)
  BODY=$(echo "$RESP" | head -1)
  
  if [ "$CODE" = "200" ]; then
    local question=$(echo "$BODY" | grep -o '"question":"[^"]*"' | cut -d'"' -f4 | head -c 60)
    if [ -n "$expected_step" ]; then
      if echo "$BODY" | grep -q "\"question_id\":\"$expected_step\""; then
        pass "→ $expected_step: $question"
      else
        fail "Expected step '$expected_step', got: $BODY"
      fi
    else
      pass "Reply OK: $question"
    fi
  else
    fail "Reply failed ($CODE): $BODY"
  fi
}

# ── Start intake ────────────────────────────────────────────────────────────

info "Starting intake flow..."
RESP=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/intake/start" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | head -1)

if [ "$CODE" = "200" ]; then
  pass "Intake started"
else
  fail "Intake start failed ($CODE): $BODY"
  exit 1
fi

# ── Step through intake ─────────────────────────────────────────────────────

info "Walking through intake steps..."

# Welcome → Name
intake_reply "start" "name"

# Name → DateOfBirth
intake_reply "TestRunner" "date_of_birth"

# DateOfBirth → Gender
intake_reply "1990-06-15" "gender"

# Gender → Experience
intake_reply "male" "experience"

# Experience → WeeklyKm
intake_reply "two_to_five_years" "weekly_km"

# WeeklyKm → Performance
intake_reply "30" "performance"

# Performance (skip) → RaceGoal
intake_reply "skip" "race_goal"

# RaceGoal → RaceDate
intake_reply "half_marathon" "race_date"

# RaceDate → TrainingDays
intake_reply "2026-10-15" "training_days"

# TrainingDays → LongRunDay
intake_reply "0,2,4,6" "long_run_day"

# LongRunDay → HeartRate
intake_reply "6" "heart_rate"

# HeartRate → Health
intake_reply "55" "health"

# Health → Summary
intake_reply "nee" "summary"

# ── Confirm → Plan generation ──────────────────────────────────────────────

info "Confirming intake and generating plan..."
RESP=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/intake/reply" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"value":"confirm"}' \
  --max-time 120)
CODE=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | head -1)

if [ "$CODE" = "200" ]; then
  if echo "$BODY" | grep -q "plan_updated\|trainingsplan\|weken"; then
    pass "Plan generated successfully!"
  else
    pass "Confirm accepted (response: ${BODY:0:100})"
  fi
else
  fail "Plan generation failed ($CODE): $BODY"
fi

# ── Verify plan exists ──────────────────────────────────────────────────────

info "Verifying plan was saved..."
RESP=$(curl -s -w "\n%{http_code}" "$API_URL/api/plans" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | head -1)

if [ "$CODE" = "200" ]; then
  if echo "$BODY" | grep -q '"weeks"'; then
    WEEKS=$(echo "$BODY" | grep -o '"week_number"' | wc -l)
    pass "Plan found with $WEEKS weeks"
  else
    fail "Plan response missing weeks: ${BODY:0:200}"
  fi
elif [ "$CODE" = "404" ]; then
  fail "No active plan found (404)"
else
  fail "Plan check failed ($CODE): $BODY"
fi

# ── Verify profile exists ──────────────────────────────────────────────────

info "Verifying profile was saved..."
RESP=$(curl -s -w "\n%{http_code}" "$API_URL/api/profiles/me" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | head -1)

if [ "$CODE" = "200" ]; then
  if echo "$BODY" | grep -q '"TestRunner"'; then
    pass "Profile found: TestRunner"
  else
    fail "Profile missing name: ${BODY:0:200}"
  fi
else
  fail "Profile check failed ($CODE): $BODY"
fi

# ── Summary ─────────────────────────────────────────────────────────────────

echo ""
if [ $ERRORS -eq 0 ]; then
  echo -e "${GREEN}All tests passed! ✅${NC}"
else
  echo -e "${RED}$ERRORS test(s) failed ❌${NC}"
fi
exit $ERRORS
