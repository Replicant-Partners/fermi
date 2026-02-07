# Zitadel Self-Hosted Setup Guide

## Quick Deploy to Railway (5 minutes)

### Option 1: Railway Dashboard (Easiest)

1. **Go to Railway:**
   - Visit https://railway.app
   - Login or sign up (free)

2. **Create New Project:**
   - Click "New Project"
   - Choose "Empty Project"

3. **Deploy Zitadel:**
   - Click "Deploy from GitHub repo"
   - Connect your GitHub account
   - Select the `fermi` repository
   - Set root directory: `/zitadel`
   - Railway will detect the Dockerfile automatically

4. **Configure Environment Variables:**
   
   Click on the service → Variables → Add these:

   ```
   ZITADEL_EXTERNALDOMAIN=<your-railway-domain>.up.railway.app
   ZITADEL_DATABASE_POSTGRES_HOST=ep-plain-term-ahgv8fhm-pooler.c-3.us-east-1.aws.neon.tech
   ZITADEL_DATABASE_POSTGRES_PORT=5432
   ZITADEL_DATABASE_POSTGRES_DATABASE=neondb
   ZITADEL_DATABASE_POSTGRES_USER_USERNAME=neondb_owner
   ZITADEL_DATABASE_POSTGRES_USER_PASSWORD=npg_wAY2hyU3eHbK
   ZITADEL_DATABASE_POSTGRES_USER_SSL_MODE=require
   ZITADEL_DATABASE_POSTGRES_ADMIN_USERNAME=neondb_owner
   ZITADEL_DATABASE_POSTGRES_ADMIN_PASSWORD=npg_wAY2hyU3eHbK
   ZITADEL_DATABASE_POSTGRES_ADMIN_SSL_MODE=require
   ZITADEL_EXTERNALSECURE=true
   ZITADEL_EXTERNALPORT=443
   ZITADEL_DEFAULTINSTANCE_ORG_NAME=ILabra
   ```

5. **Generate Public URL:**
   - Go to Settings → Networking
   - Click "Generate Domain"
   - Copy the URL: `https://your-project.up.railway.app`

6. **Wait for Deployment:**
   - Watch the logs (usually 2-3 minutes)
   - Look for "Zitadel started successfully"

### Option 2: Railway CLI

```bash
# Install Railway CLI
npm i -g @railway/cli

# Login
railway login

# Navigate to zitadel directory
cd /home/ilabra/fermi/zitadel

# Initialize project
railway init

# Deploy
railway up

# Add domain
railway domain
```

## After Deployment

### Step 1: Access Zitadel

1. Visit your Railway URL: `https://your-project.up.railway.app`
2. You'll see the Zitadel setup wizard

### Step 2: Create Admin Account

1. Click "Setup" or "Create Instance"
2. Create your admin user:
   - Email: your-email@domain.com
   - Password: (secure password)
3. Verify email (check spam folder)

### Step 3: Create Organization & Project

1. Organization Name: "ILabra" (or your company name)
2. Create Project: "Fermi"
3. Note your Organization ID

### Step 4: Create Application

1. Go to Projects → Fermi → Applications
2. Click "New Application"
3. Settings:
   - Name: "Agent Bestiary"
   - Type: Web
   - Auth Method: PKCE (recommended) or Code
   
4. Redirect URIs:
   ```
   http://localhost:3000/auth/callback
   https://fermi.systems/auth/callback
   ```

5. **Copy Credentials:**
   - Client ID: `xxxxxxxxx@fermi`
   - (If using Code flow: Client Secret)

### Step 5: Configure OAuth Providers

#### GitHub OAuth

1. **GitHub Setup:**
   - Go to: https://github.com/settings/developers
   - New OAuth App
   - Name: "Fermi Agent Bestiary"
   - Homepage: `https://fermi.systems`
   - Callback: `https://your-project.up.railway.app/ui/login/login/externalidp/callback`
   - Copy Client ID and Secret

2. **Add to Zitadel:**
   - Settings → Identity Providers → Add Provider
   - Choose "GitHub"
   - Paste Client ID and Secret
   - Scopes: `user:email read:user`
   - Save

#### Google OAuth

1. **Google Cloud Console:**
   - Go to: https://console.cloud.google.com/apis/credentials
   - Create OAuth 2.0 Client ID
   - Application type: Web application
   - Name: "Fermi Agent Bestiary"
   - Authorized redirect URI: `https://your-project.up.railway.app/ui/login/login/externalidp/callback`
   - Copy Client ID and Secret

2. **Add to Zitadel:**
   - Settings → Identity Providers → Add Provider
   - Choose "Google"
   - Paste Client ID and Secret
   - Scopes: `openid email profile`
   - Save

#### Enable Providers

1. Settings → Login Policy
2. Enable "Allow External IDPs"
3. Select GitHub and Google
4. Configure:
   - Auto-linking: Yes (link accounts with same email)
   - Force MFA: Optional
5. Save

### Step 6: Update Your Application .env

```bash
# Update /home/ilabra/fermi/.env
ZITADEL_ISSUER=https://your-project.up.railway.app
ZITADEL_CLIENT_ID=xxxxxxxxx@fermi
ZITADEL_CLIENT_SECRET=xxxxxxxxx  # Only if using Code flow
ZITADEL_REDIRECT_URI=https://fermi.systems/auth/callback
```

## Custom Domain (Optional)

### Add auth.fermi.systems

1. **In Railway:**
   - Settings → Networking → Custom Domain
   - Add: `auth.fermi.systems`

2. **In Your DNS:**
   - Add CNAME record:
     ```
     Name:  auth
     Type:  CNAME
     Value: your-project.up.railway.app
     TTL:   300
     ```

3. **Update Environment:**
   ```bash
   ZITADEL_EXTERNALDOMAIN=auth.fermi.systems
   ZITADEL_ISSUER=https://auth.fermi.systems
   ```

## Testing

### Test Authentication

```bash
# Test health
curl https://your-project.up.railway.app/debug/healthz

# Test OIDC discovery
curl https://your-project.up.railway.app/.well-known/openid-configuration

# Should return JSON with endpoints
```

### Test Login Flow

1. Visit: `https://your-project.up.railway.app/ui/console`
2. Login with admin credentials
3. Test GitHub/Google login buttons

## Troubleshooting

### Zitadel won't start

Check logs in Railway dashboard:
- Database connection errors? Verify Neon credentials
- Port already in use? Railway handles this automatically
- Masterkey error? Ensure it's 32 characters

### Can't access UI

- Wait 2-3 minutes for first startup (database initialization)
- Check Railway logs for "Zitadel started successfully"
- Verify domain is correct in ZITADEL_EXTERNALDOMAIN

### OAuth redirect errors

- Verify callback URLs match exactly in GitHub/Google
- Must use: `https://your-domain/ui/login/login/externalidp/callback`
- Check HTTPS is enabled

## Cost

- Railway: ~$5-10/month (Hobby plan)
- Neon PostgreSQL: $0 (shared with app, free tier)
- **Total: ~$10/month vs $100/month Zitadel Cloud EU!**

## Next Steps

After Zitadel is running:
1. ✅ Test login with all 3 providers (Email, GitHub, Google)
2. ✅ Copy ZITADEL_CLIENT_ID to .env
3. ✅ Build the login UI in Agent Bestiary
4. ✅ Integrate SIWE (Sign-In with Ethereum)

---

**Need help?** Check Railway logs or Zitadel documentation:
- https://zitadel.com/docs
- https://docs.railway.app
