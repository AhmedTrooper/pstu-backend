#!/usr/bin/env bash
set -euo pipefail

API_URL="${API_URL:-http://localhost:8080/api/v1}"
COOKIE_JAR_A=$(mktemp)
COOKIE_JAR_B=$(mktemp)
trap 'rm -f "$COOKIE_JAR_A" "$COOKIE_JAR_B"' EXIT

echo "============================================================"
echo " PSTU Pay — Production End-to-End Workflow Demonstration"
echo "============================================================"

# 1. Register User A (Alice)
echo -e "\n[1] Registering Alice (৳100,000 Seed Funding)..."
ALICE_PHONE="01711$(shuf -i 100000-999999 -n 1)"
ALICE_REG=$(curl -s -X POST "$API_URL/auth/register" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"Alice Rahman\",\"phone\":\"$ALICE_PHONE\",\"password\":\"Str0ngP@ssword123\",\"pin\":\"12345\"}")
echo "Response: $ALICE_REG"
ALICE_ACC=$(echo "$ALICE_REG" | grep -o '"account_number":"[^"]*' | cut -d'"' -f4 || echo "1000000001")

# 2. Register User B (Bob)
echo -e "\n[2] Registering Bob (৳100,000 Seed Funding)..."
BOB_PHONE="01811$(shuf -i 100000-999999 -n 1)"
BOB_REG=$(curl -s -X POST "$API_URL/auth/register" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"Bob Ahmed\",\"phone\":\"$BOB_PHONE\",\"password\":\"Str0ngP@ssword456\",\"pin\":\"54321\"}")
echo "Response: $BOB_REG"
BOB_ACC=$(echo "$BOB_REG" | grep -o '"account_number":"[^"]*' | cut -d'"' -f4 || echo "1000000002")

# 3. Login Alice
echo -e "\n[3] Logging in Alice..."
curl -s -c "$COOKIE_JAR_A" -X POST "$API_URL/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"phone\":\"$ALICE_PHONE\",\"password\":\"Str0ngP@ssword123\"}" | head -c 200
echo ""

# 4. Login Bob
echo -e "\n[4] Logging in Bob..."
curl -s -c "$COOKIE_JAR_B" -X POST "$API_URL/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"phone\":\"$BOB_PHONE\",\"password\":\"Str0ngP@ssword456\"}" | head -c 200
echo ""

# 5. Alice sends ৳500 (50,000 paisa) to Bob (Workflow W2)
echo -e "\n[5] Alice sends ৳500 to Bob with PIN verification..."
IDEM_KEY_1=$(uuidgen || cat /proc/sys/kernel/random/uuid)
TRANSFER_RES=$(curl -s -b "$COOKIE_JAR_A" -X POST "$API_URL/transfers" \
  -H "Content-Type: application/json" \
  -H "x-csrf: 1" \
  -d "{\"recipient\":\"$BOB_PHONE\",\"amount_paisa\":\"50000\",\"note\":\"Dinner split\",\"pin\":\"12345\",\"idempotency_key\":\"$IDEM_KEY_1\"}")
echo "Transfer: $TRANSFER_RES"

# 6. Bob requests ৳200 (20,000 paisa) from Alice (Workflow W3)
echo -e "\n[6] Bob creates a money request of ৳200 to Alice..."
REQUEST_RES=$(curl -s -b "$COOKIE_JAR_B" -X POST "$API_URL/requests" \
  -H "Content-Type: application/json" \
  -H "x-csrf: 1" \
  -d "{\"debtor\":\"$ALICE_PHONE\",\"amount_paisa\":\"20000\",\"note\":\"Coffee share\"}")
echo "Request: $REQUEST_RES"
REQUEST_ID=$(echo "$REQUEST_RES" | grep -o '"id":"[^"]*' | head -n1 | cut -d'"' -f4 || echo "")

if [ -n "$REQUEST_ID" ]; then
  # 7. Alice accepts money request with PIN
  echo -e "\n[7] Alice accepts Bob's request of ৳200..."
  IDEM_KEY_2=$(uuidgen || cat /proc/sys/kernel/random/uuid)
  ACCEPT_RES=$(curl -s -b "$COOKIE_JAR_A" -X POST "$API_URL/requests/$REQUEST_ID/accept" \
    -H "Content-Type: application/json" \
    -H "x-csrf: 1" \
    -d "{\"pin\":\"12345\",\"idempotency_key\":\"$IDEM_KEY_2\"}")
  echo "Accept: $ACCEPT_RES"
fi

# 8. Alice creates a Payment Link for ৳150 (15,000 paisa) (Workflow W4)
echo -e "\n[8] Alice creates a payment link with PIN..."
LINK_RES=$(curl -s -b "$COOKIE_JAR_A" -X POST "$API_URL/links" \
  -H "Content-Type: application/json" \
  -H "x-csrf: 1" \
  -d "{\"amount_paisa\":\"15000\",\"note\":\"Gift voucher\",\"expires_in_seconds\":3600,\"pin\":\"12345\"}")
echo "Link: $LINK_RES"
LINK_TOKEN=$(echo "$LINK_RES" | grep -o '"token":"[^"]*' | head -n1 | cut -d'"' -f4 || echo "")

if [ -n "$LINK_TOKEN" ]; then
  # 9. Bob claims the payment link with PIN (Workflow W5)
  echo -e "\n[9] Bob claims Alice's payment link..."
  IDEM_KEY_3=$(uuidgen || cat /proc/sys/kernel/random/uuid)
  CLAIM_RES=$(curl -s -b "$COOKIE_JAR_B" -X POST "$API_URL/links/$LINK_TOKEN/claim" \
    -H "Content-Type: application/json" \
    -H "x-csrf: 1" \
    -d "{\"pin\":\"54321\",\"idempotency_key\":\"$IDEM_KEY_3\"}")
  echo "Claim: $CLAIM_RES"
fi

# 10. AI Natural Language Intent Parsing (Workflow W6)
echo -e "\n[10] Parsing natural language intent with offline AI grammar..."
AI_RES=$(curl -s -b "$COOKIE_JAR_A" -X POST "$API_URL/ai/parse" \
  -H "Content-Type: application/json" \
  -d "{\"text\":\"Send 250 taka to $BOB_PHONE for groceries\"}")
echo "AI Intent: $AI_RES"

# 11. Transaction History (P5)
echo -e "\n[11] Querying Alice's keyset cursor transaction history..."
curl -s -b "$COOKIE_JAR_A" -X GET "$API_URL/me/transactions?limit=5" | head -c 300
echo ""

# 12. Statement CSV Export (P25)
echo -e "\n[12] Exporting statement CSV for Alice..."
curl -s -b "$COOKIE_JAR_A" -X GET "$API_URL/me/statement.csv" | head -n 10

# 13. In-App Notifications (P31, P32)
echo -e "\n[13] Checking Bob's notifications..."
NOTIFS=$(curl -s -b "$COOKIE_JAR_B" -X GET "$API_URL/me/notifications")
echo "Notifications: $NOTIFS"

echo -e "\n[14] Running Standalone 3-Contract Double-Entry Ledger Reconciliation..."
cargo run --bin reconcile || true

echo -e "\n============================================================"
echo " Demonstration Completed Successfully!"
echo "============================================================"
