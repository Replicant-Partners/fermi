# Agent Bestiary Deployment Checklist

## Pre-Deployment

- [x] Database setup (Neon Postgres via Vercel)
- [x] Migrations run (GitHub tracking added)
- [x] API code complete (agents.rs, health.rs)
- [x] Domains acquired (agent-bestiary.world, fermi.systems)
- [x] DNS configured (Name.com → Vercel)

## Vercel Configuration

### 1. Environment Variables

Add to Vercel project settings → Environment Variables:

```
DATABASE_URL=postgresql://neondb_owner:npg_wAY2hyU3eHbK@ep-plain-term-ahgv8fhm-pooler.c-3.us-east-1.aws.neon.tech/neondb?sslmode=require
```

**Important**: Add for all environments (Production, Preview, Development)

### 2. Custom Domains

Add in Vercel project settings → Domains:

1. **agent-bestiary.world**
   - Set as primary domain
   - Vercel will auto-provision SSL

2. **www.agent-bestiary.world**  
   - Redirect to agent-bestiary.world

3. **the-agent-bestiary.world**
   - Redirect to agent-bestiary.world

4. **fermi.systems**
   - Points to same project
   - Serves Fermi frontend

5. **www.fermi.systems**
   - Redirect to fermi.systems

### 3. Build Settings

Should auto-detect from vercel.json:
- **Framework Preset**: Other
- **Build Command**: (none - serverless functions)
- **Output Directory**: (none)
- **Install Command**: (default)

## Deployment Steps

### Step 1: Push to GitHub

```bash
git push origin main
```

This will trigger automatic Vercel deployment.

### Step 2: Verify Deployment

Wait for deployment to complete (2-5 minutes), then test:

```bash
# Health check
curl https://agent-bestiary.world/api/health

# Expected response:
{
  "status": "ok",
  "service": "agent-bestiary",
  "description": "Active Dreaming Memory backend for AI agents",
  "version": "1.0.0",
  "api_version": "v1"
}
```

### Step 3: Test Agents API

```bash
# List agents
curl https://agent-bestiary.world/api/agents

# Expected: JSON with 102 agents

# Create test agent
curl -X POST https://agent-bestiary.world/api/agents \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "test-agent",
    "agent_type": "test",
    "executor_type": "llm",
    "model": "gpt-4"
  }'

# Expected: 201 Created with agent_id
```

### Step 4: Verify Domains

Test all domain redirects:

```bash
curl -I http://agent-bestiary.world
# Should redirect to https://agent-bestiary.world

curl -I http://www.agent-bestiary.world  
# Should redirect to https://agent-bestiary.world

curl -I http://the-agent-bestiary.world
# Should redirect to https://agent-bestiary.world

curl -I https://fermi.systems
# Should return 200 OK
```

## Troubleshooting

### Issue: 500 Internal Server Error

**Check**:
1. Vercel logs (Dashboard → Deployments → View Function Logs)
2. DATABASE_URL environment variable is set
3. Database connection string is correct

### Issue: DATABASE_URL not set

**Solution**:
1. Go to Vercel Dashboard
2. Project Settings → Environment Variables
3. Add DATABASE_URL for all environments
4. Redeploy

### Issue: Domain not resolving

**Check**:
1. DNS propagation (can take 5-60 minutes)
2. Verify DNS records at Name.com
3. Check Vercel domain status (Dashboard → Domains)

### Issue: SSL certificate not provisioned

**Solution**:
1. Wait up to 2 hours for Vercel to provision
2. Ensure DNS is correctly pointing to Vercel
3. Check Vercel domain status

## Post-Deployment Verification

- [ ] Health endpoint returns 200
- [ ] Agents list endpoint returns 102 agents
- [ ] Create agent works
- [ ] agent-bestiary.world resolves
- [ ] fermi.systems resolves
- [ ] All redirects work
- [ ] HTTPS works on all domains

## Next Steps

After successful deployment:

1. Update docs with live API URLs
2. Test with Postman/Insomnia collection
3. Create sample agent for demos
4. Set up monitoring (Vercel Analytics)
5. Begin Week 1 GTM tasks (see docs/agent-bestiary/go-to-market/week-1-action-items.md)

## Rollback Plan

If deployment fails:

1. Check previous deployment in Vercel dashboard
2. Click "Redeploy" on last working deployment
3. Fix issues locally
4. Test with `vercel dev`
5. Deploy again

## Monitoring

- **Vercel Dashboard**: https://vercel.com/dashboard
- **Deployment Logs**: Project → Deployments → Logs
- **Function Logs**: Specific deployment → Function Logs
- **Analytics**: Project → Analytics

## Support

- Vercel Docs: https://vercel.com/docs
- Vercel Support: https://vercel.com/support
- Database (Neon): https://neon.tech/docs
