# Fermi — post-share roadmap

Deferred follow-ups from the "get it ready to share" sprint. Ordered by
user impact and shipping urgency. Kept concise on purpose — each item
is one to three commits of scope.

## 1. Sharing — beyond "invite accepted"

The invite pipeline is functionally complete (invite → email → landing
page → accept → materialised share), but the surrounding experience has
a few gaps.

- [ ] **Bounce / complaint webhooks.** Resend POSTs back when a send
      bounces or the recipient reports spam. Wire a
      `/api/webhooks/resend` handler that marks the invite as
      `undeliverable` and surfaces the state in the Access tab so the
      operator knows to fall back to copy-link.
- [ ] **Invite reminders.** Auto-remind pending invitees after 3 / 7
      days. Table already tracks `expires_at`; add a background job
      that sends one nudge and one expiry warning.
- [ ] **Team-invite dedicated template.** Currently team invites use
      the same HTML as forecast/portfolio. Give teams their own
      onboarding tone (member roster, team mission, existing shared
      artifacts).
- [ ] **Share dialog polish.** The Access tab is functional but
      dense. Give it (a) a "Public link" toggle for public forecasts
      with an auto-copied URL, (b) an inline avatar for users the
      forecast is already shared with, (c) an audit trail
      ("Alice invited Bob on Jul 12 · accepted Jul 13").
- [ ] **Localisation.** English only today. When we onboard our first
      non-English speaker, extract templates into a small i18n bundle
      keyed by `Accept-Language`.

## 2. Preferences

There's no user-facing preferences surface today. Everything runs on
sensible defaults. Add a dedicated `/settings` page with:

- [ ] **Notification preferences.** Which events email you: invite
      received, invite accepted, forecast shared with me, upstream
      workspace resolved, cascade queued for review. Default: all on.
      Storage: `user_preferences` JSONB per user_id.
- [ ] **Display name + avatar.** Emails currently say "550e8400-e29b…
      invited you" if the inviter has no display name. Let users set
      one from the settings page, feed it into `users.display_name`,
      cascade to the invite email templates.
- [ ] **Sender identity.** For paid tiers, let a team set a custom
      "From:" address so their invites read as "Acme Corp Fermi
      <invites@acme.com>" instead of the generic Fermi sender. Requires
      per-team domain verification via Resend's `domains` API.
- [ ] **API keys management UI.** Currently API keys are minted from a
      script; give users a settings-page card to create/revoke/list
      their keys with descriptions and last-used timestamps. Table
      `api_keys` already exists.
- [ ] **Data export / delete.** Basic GDPR: a settings-page button
      that triggers a full export of the user's forecasts / evidence /
      invites / shares, and a hard-delete flow with confirmations.

## 3. Portfolio taxonomy (in flight)

The Portfolio panel is the first surface that expresses the "portfolio
= collection of forecasts" mental model, but today it mixes named
portfolios with raw workspace forecasts and the local Lab list.
Cleanup plan tracked in the concurrent Portfolio redesign commit.

- [x] **Live is the default view.** Resolved becomes a collapsible
      archive at the bottom of each portfolio.
- [x] **Portfolio rollup metrics.** Header of each portfolio shows
      counts (live / resolved), avg + best Brier, sharing status.
- [x] **Sharing visibility per portfolio.** Chip showing which teams
      / users the portfolio is shared with; click for full breakdown.
- [ ] **Portfolio-level Brier scoring.** Aggregate the member
      forecasts' Brier scores into a portfolio score so we can
      leaderboard portfolios (not just individual forecasts).
- [ ] **Cross-portfolio tags.** Let a forecast live in multiple
      portfolios AND carry independent tags (e.g. `sports`,
      `europe`, `quarterly-review`) for filter-based views.

## 4. Schema-health follow-ups

- [ ] **`found_signatures` diagnostic** — deployed in `534f7ae` but
      not yet verified in prod. When Vercel picks it up, re-run the
      schema-health probe; if the two "missing" functions
      (`resolve_forecast`, `compute_brier_score`) actually have a
      signature drift, fix at the source of ensure_critical_schema
      declarations. If truly missing, investigate why the ensure
      block silently failed.
- [ ] **Retire the ensure_critical_schema mirror.** Long-term the
      right pattern is: migrations are authoritative, one probe
      compares actual DB state against a declared SchemaObject list,
      degraded state triggers an alert. The current parallel
      declaration in `ensure_critical_schema` is a workaround for the
      Vercel/PgBouncer multi-statement-DDL bug; it's tolerable but
      not elegant.

## 5. Trajectory viz — layer 2

The layered ribbon + hover chip landed. The next tier of
storytelling:

- [ ] **Agent lanes below the chart.** One horizontal strip per agent
      showing runs over time (History Flow's core idiom). Click a
      strip → filter events to that agent.
- [ ] **Driver attribution on hover.** When hovering a rate-revision
      dot, show "socio_capital pushed +0.4pp, form_signal pushed
      +0.6pp" — requires threading driver deltas into the
      forecast_updates writer.
- [ ] **Event labels on the chart.** Small text next to significant
      markers (rate revisions above ±5pp, all BayesOps refits) so the
      chart tells its story without needing the event list.
- [ ] **Split divergence colour.** Cyan-tinted where model > crowd,
      purple-tinted where crowd > model, so direction is readable
      from the fill alone.

## 6. Observability

- [ ] **Structured request logs.** Currently a mix of `println!` and
      `tracing::warn!`. Adopt tracing everywhere with span-based
      correlation IDs so a failed invite email + the invite DB write
      + the client's toast can be joined in the logs.
- [ ] **Health dashboard.** `/api/admin/schema-health` exists;
      generalise to a full health dashboard covering DB pool
      saturation, Resend queue depth, Gamma API rate-limit consumption,
      Vercel function duration/error rates.
- [ ] **Slack alerts.** Pipe the important events (schema-health
      degrades, invite email fails repeatedly for the same domain, PM
      observation write drops) to a Slack incoming webhook so we don't
      only find out at share-demo time.

## 7. Recently shipped (chronological, most recent first)

Anchor points so future readers can navigate the delta:

- `56990a4` — Invites: transactional email via Resend
- `9ee64a4` — Invites: shareable link + landing page
- `c9a806a` — Base rate scoped update + trajectory legend clarity
- `489ef99` — Agent runs: `"Begin."` fallback for empty user messages
- `534f7ae` — schema-health `found_signatures` diagnostic
- `e2f701b` — schema-health endpoint
- `48aa35c` — Trajectory redesign (History Flow-inspired)
- `dbd505c` — Dashboard: Live section; Portfolio: chip clarity
- `cac963b` — `fermi_market_observations` self-heals via
  `ensure_critical_schema`
- `a21d98a` — Resolve dialog + PM confidence-signal writes
- `9e2a81a` — Renormalise mutex-group cascades (fix WC 159% bug)
