# 🚀 Zitadel Deployment Checklist

## Quick Start (15 minutes total)

### ☐ Step 1: Deploy to Railway (5 min)
1. Go to https://railway.app
2. Login/Sign up
3. "New Project" → "Deploy from GitHub"
4. Select `fermi` repo → root directory: `/zitadel`
5. Wait for build (~2-3 minutes)
6. Generate domain → Get your URL

### ☐ Step 2: First Login (2 min)
1. Visit `https://your-project.up.railway.app`
2. Create admin account
3. Verify email

### ☐ Step 3: Create Application (3 min)
1. Projects → Create "Fermi"
2. Applications → New "Agent Bestiary"
3. Type: Web, Auth: PKCE
4. Redirects:
   - `http://localhost:3000/auth/callback`
   - `https://fermi.systems/auth/callback`
5. **Copy Client ID** → Save it!

### ☐ Step 4: GitHub OAuth (2 min)
1. GitHub → Settings → Developer → OAuth Apps
2. New app:
   - Callback: `https://YOUR-RAILWAY-URL/ui/login/login/externalidp/callback`
3. Copy Client ID + Secret
4. Zitadel → Identity Providers → Add GitHub
5. Paste credentials

### ☐ Step 5: Google OAuth (2 min)
1. Google Cloud Console → Credentials
2. OAuth Client ID → Web
3. Redirect: `https://YOUR-RAILWAY-URL/ui/login/login/externalidp/callback`
4. Copy Client ID + Secret
5. Zitadel → Identity Providers → Add Google
6. Paste credentials

### ☐ Step 6: Enable Providers (1 min)
1. Settings → Login Policy
2. ✅ Enable "Allow External IDPs"
3. ✅ Select GitHub & Google
4. ✅ Auto-linking: ON
5. Save

### ☐ Step 7: Update .env
```bash
# Add to /home/ilabra/fermi/.env
ZITADEL_ISSUER=https://your-project.up.railway.app
ZITADEL_CLIENT_ID=xxxxxxxxx@fermi
```

### ☐ Step 8: Test It!
1. Visit Zitadel console
2. Test login with GitHub
3. Test login with Google
4. Test login with Email

## ✅ Success Criteria

- [ ] Can access Zitadel at Railway URL
- [ ] GitHub login works
- [ ] Google login works  
- [ ] Email/password login works
- [ ] Client ID copied to .env
- [ ] Total cost: ~$10/month ✓

## 🎯 Next: Build Login UI

Once Zitadel is running, we'll create the frontend login page with all 4 auth options!

---

**Stuck?** Check `SETUP.md` for detailed instructions or Railway logs for errors.
