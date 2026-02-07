#!/bin/bash
# Quick Vercel Setup - No prompts, uses command line args
# Usage: ./vercel-quick-setup.sh <issuer> <client_id> [client_secret]

set -e

if [ $# -lt 2 ]; then
    echo "Usage: $0 <zitadel_issuer> <client_id> [client_secret]"
    echo ""
    echo "Example:"
    echo "  $0 https://myproject.up.railway.app xxxxxxxxx@fermi"
    echo "  $0 https://myproject.up.railway.app xxxxxxxxx@fermi secret123"
    exit 1
fi

ZITADEL_ISSUER="$1"
ZITADEL_CLIENT_ID="$2"
ZITADEL_CLIENT_SECRET="${3:-}"

echo "🚀 Quick Vercel Setup"
echo "===================="

# Check Vercel CLI
if ! command -v vercel &> /dev/null; then
    echo "Installing Vercel CLI..."
    npm install -g vercel
fi

cd /home/ilabra/fermi

# Add environment variables
echo "⚙️  Setting ZITADEL_ISSUER..."
vercel env add ZITADEL_ISSUER production --yes <<< "$ZITADEL_ISSUER" 2>/dev/null || \
vercel env rm ZITADEL_ISSUER production --yes && \
vercel env add ZITADEL_ISSUER production --yes <<< "$ZITADEL_ISSUER"

echo "⚙️  Setting ZITADEL_CLIENT_ID..."
vercel env add ZITADEL_CLIENT_ID production --yes <<< "$ZITADEL_CLIENT_ID" 2>/dev/null || \
vercel env rm ZITADEL_CLIENT_ID production --yes && \
vercel env add ZITADEL_CLIENT_ID production --yes <<< "$ZITADEL_CLIENT_ID"

if [ -n "$ZITADEL_CLIENT_SECRET" ]; then
    echo "⚙️  Setting ZITADEL_CLIENT_SECRET..."
    vercel env add ZITADEL_CLIENT_SECRET production --yes <<< "$ZITADEL_CLIENT_SECRET" 2>/dev/null || \
    vercel env rm ZITADEL_CLIENT_SECRET production --yes && \
    vercel env add ZITADEL_CLIENT_SECRET production --yes <<< "$ZITADEL_CLIENT_SECRET"
fi

# Set redirect URI (get current Vercel domain)
echo "⚙️  Setting ZITADEL_REDIRECT_URI..."
REDIRECT_URI="https://fermi.systems/auth/callback"
vercel env add ZITADEL_REDIRECT_URI production --yes <<< "$REDIRECT_URI" 2>/dev/null || \
vercel env rm ZITADEL_REDIRECT_URI production --yes && \
vercel env add ZITADEL_REDIRECT_URI production --yes <<< "$REDIRECT_URI"

echo ""
echo "✅ Done! Environment variables set:"
echo "  ZITADEL_ISSUER: $ZITADEL_ISSUER"
echo "  ZITADEL_CLIENT_ID: $ZITADEL_CLIENT_ID"
echo "  ZITADEL_REDIRECT_URI: $REDIRECT_URI"
echo ""
echo "🔄 Redeploy with: vercel --prod"
