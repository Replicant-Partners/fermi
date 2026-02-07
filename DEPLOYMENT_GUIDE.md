# 🚀 Complete Deployment Guide - Authentication System

## Overview

This guide walks you through deploying the complete authentication system:
1. Deploy Zitadel (auth provider) to Railway
2. Configure OAuth providers (GitHub, Google)
3. Set up Vercel environment variables
4. Build and test login UI

**Time Required:** ~30 minutes  
**Cost:** ~$10/month (Railway) vs $100/month (Zitadel Cloud EU)

---

## Phase 1: Deploy Zitadel (15 minutes)

### Step 1: Deploy to Railway

**Follow the detailed checklist:**
```bash
cd /home/ilabra/fermi/zitadel
cat DEPLOY_CHECKLIST.md
```

**Quick version:**
1. Go to https://railway.app
2. New Project → Deploy from GitHub
3. Select `fermi` repo → root: `/zitadel`
4. Wait for deployment (~3 min)
5. Generate domain → Copy URL

**Result:** You'll have `https://your-project.up.railway.app`

### Step 2: Initial Zitadel Setup

1. Visit your Railway URL
2. Create admin account
3. Verify email (check spam)
4. Create organization: "ILabra" (or your name)
5. Create project: "Fermi"

### Step 3: Create Application

1. Projects → Fermi → New Application
2. Settings:
   - Name: "Agent Bestiary"
   - Type: Web
   - Auth Method: PKCE (or Code)
3. Redirect URIs:
   ```
   http://localhost:3000/auth/callback
   https://fermi.systems/auth/callback
   ```
4. **Save and Copy:**
   - ✅ Client ID: `xxxxxxxxx@fermi`
   - ✅ Client Secret (if using Code flow)

---

## Phase 2: Configure OAuth Providers (10 minutes)

### GitHub OAuth Setup

**1. Create GitHub OAuth App:**
- Go to: https://github.com/settings/developers
- New OAuth App
- Settings:
  - Name: "Fermi Agent Bestiary"
  - Homepage: `https://fermi.systems`
  - **Callback URL:** `https://YOUR-RAILWAY-URL/ui/login/login/externalidp/callback`
- Copy Client ID and Secret

**2. Add to Zitadel:**
- Settings → Identity Providers → Add Provider
- Choose "GitHub"
- Paste credentials
- Scopes: `user:email read:user`
- Save

### Google OAuth Setup

**1. Create Google OAuth Client:**
- Go to: https://console.cloud.google.com/apis/credentials
- Create OAuth 2.0 Client ID
- Settings:
  - Type: Web application
  - Name: "Fermi Agent Bestiary"
  - **Redirect URI:** `https://YOUR-RAILWAY-URL/ui/login/login/externalidp/callback`
- Copy Client ID and Secret

**2. Add to Zitadel:**
- Settings → Identity Providers → Add Provider
- Choose "Google"
- Paste credentials
- Scopes: `openid email profile`
- Save

### Enable Providers

1. Settings → Login Policy
2. ✅ Enable "Allow External IDPs"
3. ✅ Select GitHub and Google
4. ✅ Auto-linking: Yes
5. Save

---

## Phase 3: Configure Vercel (2 minutes)

### Quick Setup (One Command)

```bash
cd /home/ilabra/fermi

# Replace with your actual values:
./scripts/vercel-quick-setup.sh \
  https://your-project.up.railway.app \
  xxxxxxxxx@fermi
```

**With client secret (if using Code flow):**
```bash
./scripts/vercel-quick-setup.sh \
  https://your-project.up.railway.app \
  xxxxxxxxx@fermi \
  your-client-secret
```

### Alternative: Interactive Setup

```bash
./scripts/vercel-setup.sh
# Follow the prompts
```

### Verify Configuration

```bash
vercel env ls | grep ZITADEL
```

You should see:
- ✅ ZITADEL_ISSUER
- ✅ ZITADEL_CLIENT_ID
- ✅ ZITADEL_REDIRECT_URI
- ✅ ZITADEL_CLIENT_SECRET (if using Code flow)

---

## Phase 4: Deploy & Test (3 minutes)

### Redeploy Vercel

```bash
cd /home/ilabra/fermi
vercel --prod
```

### Test Authentication

**1. Test Zitadel Console:**
- Visit: `https://your-project.up.railway.app/ui/console`
- Login with admin credentials
- ✅ Should work

**2. Test OAuth Providers:**
- Click "Login with GitHub" → Should redirect and work
- Click "Login with Google" → Should redirect and work
- ✅ Both should successfully authenticate

**3. Test OIDC Discovery:**
```bash
curl https://your-project.up.railway.app/.well-known/openid-configuration
```
✅ Should return JSON with endpoints

---

## Phase 5: Build Login UI (Next Step)

Once Zitadel is working, we'll create the frontend login page with:
- ⚡ Sign in with Ethereum (SIWE)
- 🐙 Sign in with GitHub
- 🔵 Sign in with Google
- 📧 Sign in with Email

---

## Troubleshooting

### Railway Issues

**Zitadel won't start:**
- Check logs: Railway Dashboard → Deployments → Logs
- Common issues:
  - Database connection: Verify Neon credentials in docker-compose.yml
  - Wait 2-3 minutes for first startup (database init)

**Can't access UI:**
- Verify domain in Railway settings
- Check ZITADEL_EXTERNALDOMAIN matches Railway URL

### Vercel Issues

**Environment variables not working:**
```bash
# Remove and re-add
vercel env rm ZITADEL_ISSUER production
vercel env add ZITADEL_ISSUER production
# Then redeploy
vercel --prod
```

**Vercel CLI not found:**
```bash
npm install -g vercel
```

### OAuth Issues

**Redirect URI mismatch:**
- Must match EXACTLY in GitHub/Google and Zitadel
- Format: `https://your-zitadel-url/ui/login/login/externalidp/callback`
- Check for trailing slashes

**Provider not showing on login:**
- Verify enabled in Settings → Login Policy
- Check Identity Providers are configured
- Ensure scopes are correct

---

## Success Checklist

Before moving to UI development, verify:

- [ ] Zitadel accessible at Railway URL
- [ ] Admin login works
- [ ] GitHub OAuth works
- [ ] Google OAuth works
- [ ] Vercel env vars configured
- [ ] OIDC discovery endpoint returns JSON
- [ ] Cost: ~$10/month ✓

---

## Cost Breakdown

| Service | Cost | Purpose |
|---------|------|---------|
| Railway (Zitadel) | ~$10/month | Auth provider |
| Neon PostgreSQL | $0 | Database (shared with app) |
| Vercel | $0 | Hosting (free tier) |
| **Total** | **~$10/month** | vs $100/month Zitadel Cloud EU |

**Annual Savings:** $1,080 🎉

---

## Next Steps

Once everything above is working:

1. **Build Login UI** - Create frontend with 4 auth buttons
2. **Integrate SIWE** - Add Ethereum wallet authentication
3. **Test End-to-End** - Verify complete auth flow
4. **Launch** 🚀

---

## Quick Reference

**Zitadel Console:**
```
https://your-project.up.railway.app/ui/console
```

**OIDC Discovery:**
```
https://your-project.up.railway.app/.well-known/openid-configuration
```

**Vercel Environment:**
```bash
vercel env ls
vercel env pull  # Download to .env.local
```

**Railway Logs:**
```bash
railway logs  # Or check Railway dashboard
```

---

## Need Help?

- **Zitadel Docs:** https://zitadel.com/docs
- **Railway Docs:** https://docs.railway.app
- **Vercel Docs:** https://vercel.com/docs/cli

**Detailed guides in this repo:**
- `zitadel/DEPLOY_CHECKLIST.md` - Step-by-step Zitadel deployment
- `zitadel/SETUP.md` - Comprehensive setup documentation
- `scripts/README.md` - Vercel CLI usage

---

**Ready to deploy? Start with Phase 1!** 🚀
