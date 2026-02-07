#!/bin/bash
# Vercel Environment Variables Setup Script
# Run this after deploying Zitadel to configure Vercel with auth credentials

set -e

echo "🚀 Vercel Environment Variables Setup"
echo "======================================"
echo ""

# Check if vercel CLI is installed
if ! command -v vercel &> /dev/null; then
    echo "❌ Vercel CLI not found. Installing..."
    npm install -g vercel
fi

echo "📝 This script will set up environment variables for authentication."
echo ""
echo "You'll need:"
echo "  1. Your Zitadel Railway URL (e.g., https://your-project.up.railway.app)"
echo "  2. Your Zitadel Client ID (e.g., xxxxxxxxx@fermi)"
echo "  3. Your Zitadel Client Secret (if using Code flow)"
echo ""

# Login to Vercel
echo "🔐 Logging in to Vercel..."
vercel login

# Link to project
echo ""
echo "🔗 Linking to Vercel project..."
vercel link

# Set environment variables
echo ""
echo "⚙️  Setting environment variables..."
echo ""

# Prompt for Zitadel credentials
read -p "Enter your Zitadel Issuer URL (e.g., https://your-project.up.railway.app): " ZITADEL_ISSUER
read -p "Enter your Zitadel Client ID (e.g., xxxxxxxxx@fermi): " ZITADEL_CLIENT_ID
read -p "Enter your Zitadel Client Secret (press Enter to skip if using PKCE): " ZITADEL_CLIENT_SECRET

# Set redirect URI based on Vercel domain
VERCEL_DOMAIN=$(vercel inspect --token $(vercel token) 2>/dev/null | grep -o 'https://[^[:space:]]*' | head -1)
ZITADEL_REDIRECT_URI="${VERCEL_DOMAIN}/auth/callback"

echo ""
echo "📤 Uploading to Vercel..."

# Set auth environment variables
vercel env add ZITADEL_ISSUER production <<< "$ZITADEL_ISSUER"
vercel env add ZITADEL_CLIENT_ID production <<< "$ZITADEL_CLIENT_ID"

if [ -n "$ZITADEL_CLIENT_SECRET" ]; then
    vercel env add ZITADEL_CLIENT_SECRET production <<< "$ZITADEL_CLIENT_SECRET"
fi

vercel env add ZITADEL_REDIRECT_URI production <<< "$ZITADEL_REDIRECT_URI"

# Also set for preview and development
vercel env add ZITADEL_ISSUER preview <<< "$ZITADEL_ISSUER"
vercel env add ZITADEL_CLIENT_ID preview <<< "$ZITADEL_CLIENT_ID"

if [ -n "$ZITADEL_CLIENT_SECRET" ]; then
    vercel env add ZITADEL_CLIENT_SECRET preview <<< "$ZITADEL_CLIENT_SECRET"
fi

echo ""
echo "✅ Environment variables configured!"
echo ""
echo "📋 Summary:"
echo "  ZITADEL_ISSUER: $ZITADEL_ISSUER"
echo "  ZITADEL_CLIENT_ID: $ZITADEL_CLIENT_ID"
echo "  ZITADEL_REDIRECT_URI: $ZITADEL_REDIRECT_URI"
echo ""
echo "🔄 Next steps:"
echo "  1. Redeploy: vercel --prod"
echo "  2. Update Zitadel redirect URIs to include: $ZITADEL_REDIRECT_URI"
echo ""
