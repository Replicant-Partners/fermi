#!/bin/bash
# Update name.com DNS records for agent-bestiary.world
# Documentation: https://www.name.com/api-docs/dns

# Set your credentials (replace with your actual values)
NAMECOM_USERNAME="${NAMECOM_USERNAME:-your_username}"
NAMECOM_API_TOKEN="${NAMECOM_API_TOKEN:-your_api_token}"
DOMAIN="agent-bestiary.world"

# Railway CNAME target
RAILWAY_CNAME="48cu8wjw.up.railway.app"

# Name.com API endpoint
API_BASE="https://api.name.com/v4"

echo "Updating DNS records for $DOMAIN..."
echo "Target: $RAILWAY_CNAME"
echo ""

# Get existing records
echo "Fetching existing DNS records..."
curl -s -u "$NAMECOM_USERNAME:$NAMECOM_API_TOKEN" \
  "$API_BASE/domains/$DOMAIN/records" \
  -H "Content-Type: application/json"

echo ""
echo ""
echo "To create/update CNAME record, run:"
echo ""
echo "curl -u \"$NAMECOM_USERNAME:$NAMECOM_API_TOKEN\" \\"
echo "  -X POST \\"
echo "  \"$API_BASE/domains/$DOMAIN/records\" \\"
echo "  -H \"Content-Type: application/json\" \\"
echo "  -d '{"
echo "    \"host\": \"@\","
echo "    \"type\": \"CNAME\","
echo "    \"answer\": \"$RAILWAY_CNAME\","
echo "    \"ttl\": 300"
echo "  }'"

echo ""
echo "Note: You need to set NAMECOM_USERNAME and NAMECOM_API_TOKEN environment variables"
echo "Get your API token from: https://www.name.com/account/settings/api"
