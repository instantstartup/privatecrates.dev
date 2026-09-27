#!/usr/bin/env bash
# Sets up Stripe for one PrivateCrates environment, idempotently:
#   - product "PrivateCrates" (fixed ID `privatecrates`)
#   - recurring price, USD 100.00 per month, lookup key `privatecrates_org_monthly`
#   - webhook endpoint https://{apex}/webhooks/stripe for the subscription events the server handles
#   - the default customer portal configuration (cancel at period end, update payment method, invoices)
# and prints the values to put in Railway.
#
# Usage:
#   STRIPE_SECRET_KEY=sk_test_... scripts/stripe-setup.sh dev
#   STRIPE_SECRET_KEY=sk_live_... scripts/stripe-setup.sh production
#   ... scripts/stripe-setup.sh dev --recreate-webhook   # new endpoint (and signing secret) if one exists
#
# dev uses test mode (sk_test_/rk_test_ keys) and production live mode (sk_live_/rk_live_); the script refuses a
# key for the wrong mode. Requires curl and jq.
#
# API reference: https://docs.stripe.com/api (products, prices, webhook_endpoints, billing_portal/configurations).

set -euo pipefail

STRIPE_API="${STRIPE_API_URL:-https://api.stripe.com}"
# Pin the API version used for these calls and for the webhook payloads. Keep it equal to the version the
# server's Stripe client sends. Latest at the time of writing: https://docs.stripe.com/upgrades
STRIPE_API_VERSION="${STRIPE_API_VERSION:-2026-08-26.dahlia}"

PRODUCT_ID="privatecrates"
LOOKUP_KEY="privatecrates_org_monthly"
UNIT_AMOUNT=10000 # cents
CURRENCY="usd"
EVENTS=(
  checkout.session.completed
  customer.subscription.created
  customer.subscription.updated
  customer.subscription.deleted
)

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
note() { printf '%s\n' "$*" >&2; }

usage() {
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

[[ $# -ge 1 ]] || usage
ENVIRONMENT="$1"
shift
RECREATE_WEBHOOK=false
for arg in "$@"; do
  case "$arg" in
    --recreate-webhook) RECREATE_WEBHOOK=true ;;
    *) usage ;;
  esac
done

case "$ENVIRONMENT" in
  dev) APEX="dev.privatecrates.dev"; MODE="test"; DASHBOARD="https://dashboard.stripe.com/test" ;;
  production | prod) ENVIRONMENT="production"; APEX="privatecrates.dev"; MODE="live"; DASHBOARD="https://dashboard.stripe.com" ;;
  *) usage ;;
esac
BASE_URL="https://${APEX}"

command -v curl >/dev/null || die "curl is required"
command -v jq >/dev/null || die "jq is required"
[[ -n "${STRIPE_SECRET_KEY:-}" ]] || die "STRIPE_SECRET_KEY is not set"
case "$STRIPE_SECRET_KEY" in
  sk_"$MODE"_* | rk_"$MODE"_*) ;;
  *) die "the $ENVIRONMENT environment needs a $MODE-mode key (sk_${MODE}_… or rk_${MODE}_…)" ;;
esac

# stripe METHOD PATH [curl data args...]: prints the JSON body; fails on a non-2xx status except 404,
# for which it prints nothing and returns 44.
stripe() {
  local method="$1" path="$2"
  shift 2
  local body status
  body="$(mktemp)"
  # The key is passed on stdin (curl -K -), so it does not appear in the process list.
  status="$(
    printf 'user = "%s:"\n' "$STRIPE_SECRET_KEY" |
      curl -sS -K - -o "$body" -w '%{http_code}' \
        -X "$method" \
        -H "Stripe-Version: ${STRIPE_API_VERSION}" \
        "$@" \
        "${STRIPE_API}${path}"
  )" || { rm -f "$body"; die "request failed: $method $path"; }
  if [[ "$status" == 404 ]]; then
    rm -f "$body"
    return 44
  fi
  if [[ "$status" != 2* ]]; then
    note "Stripe returned HTTP $status for $method $path:"
    jq -r '.error.message // .' "$body" >&2 || cat "$body" >&2
    rm -f "$body"
    exit 1
  fi
  cat "$body"
  rm -f "$body"
}

note "Stripe setup for $ENVIRONMENT ($MODE mode), apex $BASE_URL, API version $STRIPE_API_VERSION"

# --- Product ----------------------------------------------------------------------------------------
if product="$(stripe GET "/v1/products/${PRODUCT_ID}")"; then
  note "product: $PRODUCT_ID exists"
  if [[ "$(jq -r .active <<<"$product")" != true ]]; then
    stripe POST "/v1/products/${PRODUCT_ID}" -d active=true >/dev/null
    note "product: reactivated"
  fi
else
  [[ $? -eq 44 ]] || exit 1
  stripe POST /v1/products \
    -d id="$PRODUCT_ID" \
    -d name="PrivateCrates" \
    --data-urlencode description="Hosted private Cargo registry for one GitHub organisation, unlimited users." \
    --data-urlencode url="$BASE_URL" \
    -d "metadata[app]=privatecrates" >/dev/null
  note "product: created $PRODUCT_ID"
fi

# --- Price ------------------------------------------------------------------------------------------
price="$(stripe GET /v1/prices -G -d "lookup_keys[]=${LOOKUP_KEY}" -d active=true -d limit=1 | jq -c '.data[0] // empty')"
if [[ -n "$price" ]]; then
  PRICE_ID="$(jq -r .id <<<"$price")"
  note "price: $LOOKUP_KEY exists ($PRICE_ID)"
  actual="$(jq -r '"\(.unit_amount) \(.currency) \(.recurring.interval) \(.recurring.interval_count) \(.product)"' <<<"$price")"
  if [[ "$actual" != "$UNIT_AMOUNT $CURRENCY month 1 $PRODUCT_ID" ]]; then
    note "WARNING: price $PRICE_ID is '$actual', expected '$UNIT_AMOUNT $CURRENCY month 1 $PRODUCT_ID'."
    note "         Prices are immutable. Create a new price and move the lookup key to it with transfer_lookup_key=true."
  fi
else
  PRICE_ID="$(
    stripe POST /v1/prices \
      -d product="$PRODUCT_ID" \
      -d unit_amount="$UNIT_AMOUNT" \
      -d currency="$CURRENCY" \
      -d "recurring[interval]=month" \
      -d lookup_key="$LOOKUP_KEY" \
      -d nickname="Per organisation, monthly" \
      -d "metadata[app]=privatecrates" | jq -r .id
  )"
  note "price: created $PRICE_ID"
fi

# --- Webhook endpoint -------------------------------------------------------------------------------
WEBHOOK_URL="${BASE_URL}/webhooks/stripe"
event_args=()
for e in "${EVENTS[@]}"; do event_args+=(-d "enabled_events[]=$e"); done

WEBHOOK_SECRET=""
existing="$(stripe GET /v1/webhook_endpoints -G -d limit=100 | jq -r --arg url "$WEBHOOK_URL" '[.data[] | select(.url == $url)][0].id // empty')"
if [[ -n "$existing" && "$RECREATE_WEBHOOK" == true ]]; then
  stripe DELETE "/v1/webhook_endpoints/${existing}" >/dev/null
  note "webhook: deleted $existing"
  existing=""
fi
if [[ -n "$existing" ]]; then
  stripe POST "/v1/webhook_endpoints/${existing}" "${event_args[@]}" -d disabled=false >/dev/null
  note "webhook: $existing exists for $WEBHOOK_URL; events updated."
  note "         Its signing secret is only shown at creation: reveal it in ${DASHBOARD}/webhooks/${existing}"
  note "         or re-run with --recreate-webhook (then update STRIPE_WEBHOOK_SECRET in Railway)."
else
  created="$(
    stripe POST /v1/webhook_endpoints \
      --data-urlencode url="$WEBHOOK_URL" \
      "${event_args[@]}" \
      -d api_version="$STRIPE_API_VERSION" \
      --data-urlencode description="PrivateCrates ${ENVIRONMENT}: subscription state" \
      -d "metadata[app]=privatecrates"
  )"
  WEBHOOK_SECRET="$(jq -r .secret <<<"$created")"
  note "webhook: created $(jq -r .id <<<"$created") for $WEBHOOK_URL"
fi

# --- Customer portal --------------------------------------------------------------------------------
# Portal sessions created without `configuration` use the account's default configuration, which Stripe
# creates the first time the portal settings are saved in the Dashboard; the API cannot mark one as default.
portal_args=(
  --data-urlencode "business_profile[headline]=PrivateCrates billing"
  --data-urlencode "business_profile[privacy_policy_url]=${BASE_URL}/legal/privacy"
  --data-urlencode "business_profile[terms_of_service_url]=${BASE_URL}/legal/terms"
  --data-urlencode "default_return_url=${BASE_URL}/account"
  -d "features[subscription_cancel][enabled]=true"
  -d "features[subscription_cancel][mode]=at_period_end"
  -d "features[subscription_cancel][proration_behavior]=none"
  -d "features[payment_method_update][enabled]=true"
  -d "features[invoice_history][enabled]=true"
  -d "features[subscription_update][enabled]=false"
  -d "metadata[app]=privatecrates"
)
default_portal="$(stripe GET /v1/billing_portal/configurations -G -d is_default=true -d limit=1 | jq -r '.data[0].id // empty')"
if [[ -n "$default_portal" ]]; then
  stripe POST "/v1/billing_portal/configurations/${default_portal}" "${portal_args[@]}" >/dev/null
  note "portal: updated default configuration $default_portal"
else
  note "portal: this account has no default portal configuration yet."
  note "        Open ${DASHBOARD}/settings/billing/portal, click Save once, then re-run this script."
fi

# --- Output -----------------------------------------------------------------------------------------
railway_env="$ENVIRONMENT"
cat <<EOF

Railway variables for the '${railway_env}' environment:

  STRIPE_PRICE_ID=${PRICE_ID}
  STRIPE_WEBHOOK_SECRET=${WEBHOOK_SECRET:-<unchanged: see the note above>}
  STRIPE_SECRET_KEY=<the ${MODE}-mode key you ran this with, or a restricted key>

Set them with (the secret key is piped from your environment rather than typed):

  railway variable set STRIPE_PRICE_ID=${PRICE_ID} -e ${railway_env} -s privatecrates --skip-deploys
EOF
if [[ -n "$WEBHOOK_SECRET" ]]; then
  cat <<EOF
  printf '%s' '${WEBHOOK_SECRET}' | railway variable set STRIPE_WEBHOOK_SECRET --stdin -e ${railway_env} -s privatecrates --skip-deploys
EOF
fi
cat <<EOF
  printf '%s' "\$STRIPE_SECRET_KEY" | railway variable set STRIPE_SECRET_KEY --stdin -e ${railway_env} -s privatecrates
EOF
