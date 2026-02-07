# Vercel CLI Setup Scripts

## Quick Setup (Recommended)

After deploying Zitadel to Railway, use this one-liner:

```bash
cd /home/ilabra/fermi
./scripts/vercel-quick-setup.sh <RAILWAY_URL> <CLIENT_ID>
```

**Example:**
```bash
./scripts/vercel-quick-setup.sh \
  https://fermi-zitadel.up.railway.app \
  abc123xyz@fermi
```

**With client secret (if using Code flow):**
```bash
./scripts/vercel-quick-setup.sh \
  https://fermi-zitadel.up.railway.app \
  abc123xyz@fermi \
  your-secret-here
```

## Interactive Setup

For guided setup with prompts:

```bash
./scripts/vercel-setup.sh
```

This will:
1. Login to Vercel
2. Link to your project
3. Prompt for credentials
4. Set all environment variables
5. Show summary

## Manual Vercel CLI Commands

If you prefer to do it manually:

```bash
# Login
vercel login

# Link project
cd /home/ilabra/fermi
vercel link

# Add environment variables
vercel env add ZITADEL_ISSUER production
# Paste: https://your-project.up.railway.app

vercel env add ZITADEL_CLIENT_ID production
# Paste: xxxxxxxxx@fermi

vercel env add ZITADEL_REDIRECT_URI production
# Paste: https://fermi.systems/auth/callback

# Optional: Client secret
vercel env add ZITADEL_CLIENT_SECRET production
# Paste: your-secret

# Redeploy
vercel --prod
```

## Environment Variables Set

These scripts configure:

| Variable | Example | Description |
|----------|---------|-------------|
| `ZITADEL_ISSUER` | `https://auth.fermi.systems` | Your Zitadel instance URL |
| `ZITADEL_CLIENT_ID` | `abc123@fermi` | Application client ID from Zitadel |
| `ZITADEL_CLIENT_SECRET` | `secret123` | Client secret (optional, for Code flow) |
| `ZITADEL_REDIRECT_URI` | `https://fermi.systems/auth/callback` | OAuth callback URL |

## Verifying Setup

```bash
# List all environment variables
vercel env ls

# Pull environment variables to local
vercel env pull

# Check specific variable
vercel env ls | grep ZITADEL
```

## Troubleshooting

### "Vercel CLI not found"
```bash
npm install -g vercel
```

### "Not linked to a project"
```bash
cd /home/ilabra/fermi
vercel link
```

### "Environment variable already exists"
Remove it first:
```bash
vercel env rm ZITADEL_ISSUER production
vercel env add ZITADEL_ISSUER production
```

### Update existing variables
```bash
# Remove old value
vercel env rm VARIABLE_NAME production

# Add new value
vercel env add VARIABLE_NAME production
```

## After Setup

1. **Redeploy:**
   ```bash
   vercel --prod
   ```

2. **Update Zitadel redirect URIs:**
   - Go to Zitadel Console
   - Projects → Fermi → Applications → Agent Bestiary
   - Add redirect URI: `https://fermi.systems/auth/callback`
   - Save

3. **Test:**
   - Visit `https://fermi.systems`
   - Click login button
   - Should redirect to Zitadel

## Complete Workflow

```bash
# 1. Deploy Zitadel to Railway (see zitadel/DEPLOY_CHECKLIST.md)

# 2. Get credentials from Zitadel console

# 3. Configure Vercel
cd /home/ilabra/fermi
./scripts/vercel-quick-setup.sh \
  https://your-zitadel.up.railway.app \
  your-client-id@fermi

# 4. Redeploy
vercel --prod

# 5. Update Zitadel redirect URIs

# 6. Test login!
```

## Need Help?

- Vercel CLI docs: https://vercel.com/docs/cli
- Zitadel docs: https://zitadel.com/docs
- Check logs: `vercel logs`
