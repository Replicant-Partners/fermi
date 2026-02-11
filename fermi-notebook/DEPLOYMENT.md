# Fermi Notebook - Railway Deployment Instructions

## Option 1: Deploy via Railway Dashboard (Recommended)

1. **Go to Railway Dashboard**: https://railway.app/dashboard
2. **Select Project**: `agent-bestiary` (or create new project "fermi-systems")
3. **Add New Service**:
   - Click "+ New"
   - Select "GitHub Repo"
   - Choose: `Replicant-Partners/fermi`
   - **Root Directory**: `/fermi-notebook`
4. **Configure Service**:
   - Name: `fermi-notebook`
   - Build settings:
     - Builder: Dockerfile
     - Dockerfile Path: `Dockerfile` (relative to root directory)
5. **Add Environment Variables**:
   ```
   VITE_API_BASE_URL=https://agent-bestiary.world
   ```
6. **Deploy Settings**:
   - Port: 8080
   - Health Check Path: `/` (optional)
7. **Custom Domain**:
   - Add domain: `fermi.systems`
   - Or subdomain: `notebook.agent-bestiary.world`

## Option 2: Deploy via Railway CLI

```bash
cd /home/ilabra/fermi/fermi-notebook

# Link to Railway project (select agent-bestiary)
railway link

# Create new service (via dashboard, then link)
# After service is created in dashboard:
railway service link

# Set environment variables
railway variables set VITE_API_BASE_URL=https://agent-bestiary.world

# Deploy
railway up
```

## Environment Variables Required

- `VITE_API_BASE_URL`: Backend API URL (https://agent-bestiary.world)

## Build Process

1. **Build Stage** (Node 18):
   - `npm ci` - Install dependencies
   - `npm run build` - Build Svelte app to `/dist`

2. **Runtime Stage** (Nginx Alpine):
   - Serves static files from `/usr/share/nginx/html`
   - SPA routing configured in `nginx.conf`
   - Port 8080 exposed

## Post-Deployment

1. **Verify deployment**:
   - Check Railway logs for build success
   - Visit the generated Railway URL
   - Test API connectivity to agent-bestiary.world

2. **DNS Configuration** (for fermi.systems):
   ```
   Type: CNAME
   Name: @
   Value: <railway-url>.up.railway.app
   ```

## Troubleshooting

### Build fails
- Check `package.json` has all dependencies
- Verify `npm run build` works locally
- Check Railway logs for specific error

### App loads but can't connect to API
- Verify `VITE_API_BASE_URL` is set correctly
- Check CORS settings on agent-bestiary backend
- Check browser console for errors

### 404 on routes
- Verify `nginx.conf` is copied correctly
- Check `try_files $uri $uri/ /index.html;` is present

## Current Status

✅ Deployment files ready:
- `Dockerfile` - Multi-stage build
- `nginx.conf` - SPA routing + security headers
- `railway.toml` - Railway configuration
- `.env.production` - Production environment

🚧 Needs manual deployment via Railway dashboard
