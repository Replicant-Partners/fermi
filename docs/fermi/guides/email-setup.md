# Email Setup (Fermi Console → Invites)

The Fermi Console sends transactional email — currently just team / forecast /
portfolio invite links — via [Resend](https://resend.com). The modal that says
**"Email delivery not configured on the server. Share this link with … directly"**
appears when the server was booted without `RESEND_API_KEY`. Everything else
(the invite row, the token, the copy-link fallback) already works; the only
missing piece is the outbound send.

This guide walks through switching that on end-to-end.

## What you need

| Item | Where | Cost |
| ---- | ----- | ---- |
| Resend account | https://resend.com | Free tier: 3k / month, 100 / day |
| A verified sending domain | Resend dashboard → Domains | Free |
| DNS access for that domain | Name.com (or wherever your domain lives) | — |

We recommend using `agent-bestiary.world` as the sending domain since that's the
same host as `APP_BASE_URL` in production — recipients then see invites from
`no-reply@agent-bestiary.world`, which matches the invite link domain and keeps
spam scores healthy.

## 1. Verify the sending domain in Resend

1. Sign in at [resend.com](https://resend.com).
2. **Domains → Add Domain** → enter `agent-bestiary.world`.
3. Resend shows a table of DNS records (MX, TXT for SPF, one or two TXT/CNAME
   for DKIM, and optionally DMARC).
4. Add each record verbatim at Name.com → *Manage DNS* for
   `agent-bestiary.world`. Use TTL 300 while you're testing.
5. Back in Resend, click **Verify**. Propagation is usually under two minutes
   with TTL 300; occasionally up to an hour.

Do not skip verification — Resend rejects sends from unverified domains with a
`403` and the api-server logs the failure at `warn!` under
`invite email delivery failed`.

## 2. Create an API key

1. Resend dashboard → **API Keys → Create API Key**.
2. Name it `fermi-prod` (or `fermi-preview` for a Vercel preview key — you can
   scope keys per environment if you want the audit trail).
3. Permission: **Sending access** is enough. Full access is not required.
4. Copy the `re_...` value. Resend only shows it once.

## 3. Set the env vars

The api-server reads three variables at boot (`src/email.rs::EmailConfig::from_env`):

| Var | Required | Default | Notes |
| --- | -------- | ------- | ----- |
| `RESEND_API_KEY` | yes | — | The `re_...` value from step 2. Empty = no-op mode. |
| `RESEND_FROM_EMAIL` | no | `Fermi <no-reply@agent-bestiary.world>` | Must live on the verified domain. |
| `APP_BASE_URL` | no | `https://agent-bestiary.world` | Used to build invite URLs in email bodies. |

### Railway (api-server)

The api-server runs on Railway (see `Dockerfile`), so this is the important one.

```
Railway dashboard → fermi api-server service → Variables → New Variable
  RESEND_API_KEY      = re_xxxxxxxxxxxxxxxx
  RESEND_FROM_EMAIL   = Fermi <no-reply@agent-bestiary.world>
  APP_BASE_URL        = https://agent-bestiary.world
```

Railway redeploys automatically on variable change. Once up, the boot log
should print:

```
Email configured (Resend transactional delivery enabled)
```

instead of the `Note: RESEND_API_KEY not set …` line.

### Vercel (frontend / edge)

Only needed if a Vercel-hosted route ever needs to send email directly (today
it doesn't — the frontend calls the Railway api-server for invites). Adding it
now is cheap and future-proofs the `vercel.json` env references:

```
vercel env add RESEND_API_KEY production
vercel env add RESEND_FROM_EMAIL production
vercel env add APP_BASE_URL production
```

(Or via the Vercel dashboard → Project → Settings → Environment Variables.)

The env-var names line up with the `@resend_api_key` / `@resend_from_email` /
`@app_base_url` secret refs already declared in `vercel.json`.

### Local dev (`.env`)

For local console testing you can either:

- **Skip email.** The console keeps working via the copy-link modal exactly as
  it does in production without the key. This is the default and usually the
  right choice for local dev.
- **Use a Resend sandbox key.** Create a separate API key and set
  `RESEND_FROM_EMAIL="onboarding@resend.dev"` (a Resend-owned domain that is
  pre-verified and only delivers to the email address on your Resend account —
  safe for local testing without accidentally emailing real users).

```env
# .env (local)
RESEND_API_KEY="re_dev_xxxxxxxxxxxxxxxx"
RESEND_FROM_EMAIL="onboarding@resend.dev"
APP_BASE_URL="http://localhost:3000"
```

## 4. Verify end-to-end

1. Restart the api-server. Confirm the boot line
   `Email configured (Resend transactional delivery enabled)`.
2. In the Fermi Console, open **Teams → axollotl → + Invite** (or any invite
   entry point).
3. Enter a real inbox you control.
4. Expected behaviour:
   - The modal now reads **"Invite emailed to … — you can also copy this link"**
     instead of the yellow "not configured" warning.
   - The recipient gets an email within a few seconds with a dark-themed
     invite card and an "Accept invitation" button pointing at
     `${APP_BASE_URL}/invites/{token}`.
   - Resend dashboard → **Logs** shows the send with a `delivered` status.
5. If it fails: `railway logs --service api-server | grep -i invite` will show
   the `warn!` line with the exact Resend error (usually a 403 because the
   sending domain isn't verified yet, or the `from` address doesn't match a
   verified domain).

## Failure semantics (for reference)

Email sends are **fire-and-forget** — the invite row is committed to the DB
*before* the Resend call is spawned (`EmailConfig::spawn_invite_email` in
`src/email.rs`). A Resend failure therefore never rolls back the invite; the
owner can still hand the recipient the copy-link fallback from the same modal.
This is deliberate: transient Resend errors shouldn't lose invite tokens.

## Where the code lives

- `src/email.rs` — `EmailConfig`, templates, Resend HTTP call.
- `src/handlers/invites.rs::create_invite_row` — spawns the send and returns
  the `email_sent` flag to the client.
- `crates/fermi-console/src/main.rs::render_invite_share_modal` — the modal
  copy that changes based on `email_sent`.