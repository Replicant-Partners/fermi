# Vercel Domain Setup Guide

## Domains
- **Primary**: agent-bestiary.world
- **Redirect**: the-agent-bestiary.world → agent-bestiary.world

## Step 1: Add Domains to Vercel

1. Go to Vercel Dashboard → Your Project → Settings → Domains
2. Click "Add Domain"
3. Add `agent-bestiary.world`
4. Add `the-agent-bestiary.world`

## Step 2: Configure DNS

Vercel will provide DNS records to configure. You'll need to add these to your domain registrar:

### For apex domain (agent-bestiary.world)

**Option A: Use Vercel nameservers (recommended)**
```
ns1.vercel-dns.com
ns2.vercel-dns.com
```

**Option B: Use A records**
```
Type: A
Name: @
Value: 76.76.21.21
TTL: 300
```

### For subdomain (the-agent-bestiary.world)

```
Type: CNAME
Name: the-agent-bestiary
Value: cname.vercel-dns.com
TTL: 300
```

### For www subdomain (optional)

```
Type: CNAME
Name: www
Value: cname.vercel-dns.com
TTL: 300
```

## Step 3: Set Primary Domain

In Vercel Dashboard → Settings → Domains:
1. Find `agent-bestiary.world`
2. Click the three dots → "Set as Primary"

## Step 4: Configure Redirects

In Vercel Dashboard → Settings → Domains:
1. Find `the-agent-bestiary.world`
2. Click "Redirect to agent-bestiary.world"
3. Select "Permanent (308)" for SEO

Also redirect www (if configured):
1. Find `www.agent-bestiary.world`
2. Redirect to `agent-bestiary.world`

## Step 5: Enable HTTPS

Vercel automatically provisions SSL certificates. Wait 1-2 hours for:
- DNS propagation
- SSL certificate issuance
- HTTPS to become available

## Step 6: Verify

Test all URLs redirect correctly:
```bash
# Should all redirect to https://agent-bestiary.world
curl -I http://agent-bestiary.world
curl -I https://agent-bestiary.world
curl -I http://the-agent-bestiary.world
curl -I https://the-agent-bestiary.world
curl -I http://www.agent-bestiary.world  # if configured
```

## Expected Timeline

- **Immediate**: Domains added to Vercel
- **5-10 minutes**: DNS records configured at registrar
- **30-60 minutes**: DNS propagation globally
- **1-2 hours**: SSL certificate issued
- **2-4 hours**: Fully operational with HTTPS

## Troubleshooting

### Domain not resolving
- Wait for DNS propagation (up to 48 hours, usually <1 hour)
- Check DNS records at registrar match Vercel requirements
- Use `dig agent-bestiary.world` to verify DNS

### SSL certificate not issued
- Verify DNS is correctly configured
- Wait 2-4 hours after DNS propagation
- Check Vercel Dashboard for certificate status

### Redirect not working
- Ensure redirect is configured in Vercel Dashboard
- Clear browser cache
- Try incognito/private browsing mode

## References

- [Vercel Domains Documentation](https://vercel.com/docs/concepts/projects/domains)
- [Vercel DNS Configuration](https://vercel.com/docs/concepts/projects/domains/dns)
- [Check DNS Propagation](https://www.whatsmydns.net/)
