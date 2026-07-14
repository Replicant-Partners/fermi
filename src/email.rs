//! Outbound email delivery via Resend.
//!
//! Kept deliberately provider-specific rather than abstracting over an
//! "EmailProvider" trait: we send at most a handful of transactional
//! templates (invites today, notifications later), and picking Resend
//! for both the free tier and the developer ergonomics means the code
//! stays a hundred lines instead of an SMTP-plus-adapters framework.
//!
//! ## Configuration
//!
//! Two env vars govern behaviour:
//!
//!   - `RESEND_API_KEY` — the API key from resend.com. When missing or
//!     empty the client is `unconfigured`, and every `send_*` call
//!     silently logs "email delivery skipped: no API key" instead of
//!     erroring. This is deliberate: local dev + CI must not require
//!     credentials, and copy-link fallback in the console already gives
//!     operators a working share path without email.
//!   - `RESEND_FROM_EMAIL` — sender address. Must be on a domain
//!     verified in the Resend dashboard. Defaults to
//!     `Fermi <no-reply@agent-bestiary.world>` if unset — you'll want
//!     to override once the sender domain is verified.
//!   - `APP_BASE_URL` — used to construct invite links. Defaults to
//!     `https://agent-bestiary.world` in prod-shaped deploys.
//!
//! ## Failure semantics
//!
//! Send calls **never fail the enclosing request**. The invite row is
//! already committed by the time we reach `send_invite_email`; if the
//! email bounces we don't want to unwind the DB write. Failures are
//! logged at `warn!` with the invite id + recipient so operators can
//! diagnose and, worst case, hand the recipient the copy-link fallback.
//!
//! ## What's NOT here
//!
//! Bounce / complaint webhooks, retry queues, template versioning,
//! localisation — these are all follow-ups when we outgrow "handful of
//! transactional emails". For now the templates are inline HTML+text.
//!
//! Compatible with Resend's API v1 (`POST https://api.resend.com/emails`).

use serde::Serialize;

/// Configuration for outbound email. Cloned into [`AppState`], so it's
/// cheap (three short strings + a reqwest::Client which is Arc-wrapped
/// internally).
#[derive(Clone)]
pub struct EmailConfig {
    api_key: String,
    from: String,
    app_base_url: String,
    client: reqwest::Client,
}

impl EmailConfig {
    /// Build from env. Never panics — an unset API key produces an
    /// `unconfigured` client whose `send_*` calls become no-ops.
    pub fn from_env() -> Self {
        let api_key = std::env::var("RESEND_API_KEY").unwrap_or_default();
        let from = std::env::var("RESEND_FROM_EMAIL")
            .unwrap_or_else(|_| "Fermi <no-reply@agent-bestiary.world>".to_string());
        let app_base_url = std::env::var("APP_BASE_URL")
            .unwrap_or_else(|_| "https://agent-bestiary.world".to_string());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            api_key,
            from,
            app_base_url,
            client,
        }
    }

    /// True when the API key is set. Handlers can gate on this to
    /// surface "email skipped" telemetry.
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    /// Absolute URL to the invite landing page for a given token.
    pub fn invite_url(&self, token: &str) -> String {
        format!(
            "{}/invites/{}",
            self.app_base_url.trim_end_matches('/'),
            token
        )
    }

    /// Fire-and-forget invite email. See [`InviteEmailArgs`] for the
    /// input shape. Returns `Ok(())` on 2xx, `Err(msg)` otherwise.
    /// Callers typically spawn this and log the result, since the DB
    /// state doesn't depend on delivery success.
    pub async fn send_invite_email(&self, args: InviteEmailArgs<'_>) -> Result<(), String> {
        if !self.is_configured() {
            return Err("no API key configured".into());
        }

        let subject = format!(
            "{} invited you to a {} on Fermi",
            args.inviter_display, args.target_type
        );

        let permission_label = match args.permission {
            "view" => "view",
            "edit" => "edit",
            "admin" => "co-own",
            "owner" => "own",
            "member" => "join as a member of",
            "viewer" => "view as a member of",
            other => other,
        };

        let html_body = format_invite_html(
            args.inviter_display,
            args.target_type,
            permission_label,
            args.invite_url,
            args.message,
            args.expires_at,
        );
        let text_body = format_invite_text(
            args.inviter_display,
            args.target_type,
            permission_label,
            args.invite_url,
            args.message,
            args.expires_at,
        );

        let body = ResendSendRequest {
            from: &self.from,
            to: [args.recipient_email],
            subject: &subject,
            html: &html_body,
            text: &text_body,
        };

        let resp = self
            .client
            .post("https://api.resend.com/emails")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("resend request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "resend rejected send (HTTP {}): {}",
                status,
                text.chars().take(200).collect::<String>()
            ));
        }
        Ok(())
    }
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

/// Everything the invite template needs. Kept as borrowed refs so the
/// handler doesn't need to clone the whole invite row.
pub struct InviteEmailArgs<'a> {
    pub recipient_email: &'a str,
    pub inviter_display: &'a str,
    pub target_type: &'a str,
    pub permission: &'a str,
    pub invite_url: &'a str,
    pub message: Option<&'a str>,
    pub expires_at: &'a str,
}

/// Owned variant used by [`EmailConfig::spawn_invite_email`] so the
/// spawned task doesn't borrow from the request scope. Cheap to build
/// — a handful of short Strings — and avoids threading lifetimes into
/// tokio::spawn.
pub struct OwnedInviteEmailArgs {
    pub recipient_email: String,
    pub inviter_display: String,
    pub target_type: String,
    pub permission: String,
    pub invite_url: String,
    pub message: Option<String>,
    pub expires_at: String,
}

impl OwnedInviteEmailArgs {
    fn as_borrowed(&self) -> InviteEmailArgs<'_> {
        InviteEmailArgs {
            recipient_email: &self.recipient_email,
            inviter_display: &self.inviter_display,
            target_type: &self.target_type,
            permission: &self.permission,
            invite_url: &self.invite_url,
            message: self.message.as_deref(),
            expires_at: &self.expires_at,
        }
    }
}

impl EmailConfig {
    /// Fire-and-forget wrapper around [`send_invite_email`]. Spawns a
    /// tokio task so the caller (typically a request handler that has
    /// already committed the invite row to the DB) returns immediately
    /// regardless of Resend latency or failure.
    pub fn spawn_invite_email(&self, args: OwnedInviteEmailArgs) {
        let cfg = self.clone();
        tokio::spawn(async move {
            if let Err(e) = cfg.send_invite_email(args.as_borrowed()).await {
                tracing::warn!(
                    recipient = %args.recipient_email,
                    error = %e,
                    "invite email delivery failed — operator can fall back to copy-link"
                );
            } else {
                tracing::info!(
                    recipient = %args.recipient_email,
                    "invite email delivered"
                );
            }
        });
    }
}

// ─── Resend API payload ────────────────────────────────────────────

#[derive(Serialize)]
struct ResendSendRequest<'a> {
    from: &'a str,
    to: [&'a str; 1],
    subject: &'a str,
    html: &'a str,
    text: &'a str,
}

// ─── Templates ─────────────────────────────────────────────────────

fn format_invite_html(
    inviter: &str,
    target_type: &str,
    permission_label: &str,
    invite_url: &str,
    message: Option<&str>,
    expires_at: &str,
) -> String {
    let msg_block = message
        .filter(|m| !m.trim().is_empty())
        .map(|m| {
            format!(
                r#"<div style="margin: 20px 0; padding: 12px 16px; background: #272d38; border-left: 3px solid #5ccfe6; border-radius: 4px; color: #cbccc6; font-size: 14px; line-height: 1.5;">{}</div>"#,
                escape_html(m)
            )
        })
        .unwrap_or_default();

    format!(
        r#"<!doctype html>
<html>
<body style="margin: 0; padding: 0; background: #1f2430; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; color: #cbccc6;">
  <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background: #1f2430;">
    <tr>
      <td align="center" style="padding: 32px 16px;">
        <table role="presentation" width="480" cellpadding="0" cellspacing="0" style="background: #272d38; border: 1px solid #3e4b59; border-radius: 12px; padding: 32px;">
          <tr><td>
            <div style="font-size: 12px; color: #5c6773; margin-bottom: 8px; letter-spacing: 1px;">FERMI</div>
            <h1 style="font-size: 22px; margin: 0 0 8px 0; color: #cbccc6; font-weight: 600;">You've been invited</h1>
            <p style="font-size: 15px; color: #cbccc6; margin: 0 0 20px 0; line-height: 1.5;">
              <strong>{inviter}</strong> invited you to {permission} a {target}.
            </p>
            {msg_block}
            <div style="margin: 28px 0;">
              <a href="{url}" style="display: inline-block; background: #5ccfe6; color: #1f2430; padding: 12px 24px; border-radius: 8px; font-weight: 600; text-decoration: none; font-size: 14px;">Accept invitation</a>
            </div>
            <p style="font-size: 12px; color: #5c6773; margin: 20px 0 0 0;">
              This invite expires on {expires}. If the button doesn't work, paste this link into your browser:
            </p>
            <p style="font-size: 12px; color: #5ccfe6; margin: 8px 0 0 0; word-break: break-all;">
              <a href="{url}" style="color: #5ccfe6;">{url}</a>
            </p>
          </td></tr>
        </table>
        <p style="font-size: 11px; color: #3e4b59; margin: 16px 0 0 0;">
          Fermi · Forecasting Command Center
        </p>
      </td>
    </tr>
  </table>
</body>
</html>"#,
        inviter = escape_html(inviter),
        permission = escape_html(permission_label),
        target = escape_html(target_type),
        msg_block = msg_block,
        url = escape_html(invite_url),
        expires = escape_html(expires_at),
    )
}

fn format_invite_text(
    inviter: &str,
    target_type: &str,
    permission_label: &str,
    invite_url: &str,
    message: Option<&str>,
    expires_at: &str,
) -> String {
    let mut s = String::new();
    s.push_str("You've been invited\n\n");
    s.push_str(&format!(
        "{} invited you to {} a {}.\n\n",
        inviter, permission_label, target_type
    ));
    if let Some(m) = message.filter(|m| !m.trim().is_empty()) {
        s.push_str("Message:\n");
        s.push_str(m);
        s.push_str("\n\n");
    }
    s.push_str("Accept the invitation:\n");
    s.push_str(invite_url);
    s.push_str(&format!("\n\nThis invite expires on {}.\n", expires_at));
    s.push_str("\n— Fermi\n");
    s
}

/// Minimal HTML escape — enough to prevent injection through
/// inviter names, custom messages, and the invite URL itself. We stay
/// with an inline function rather than pulling `html_escape` because
/// this is the only spot in the codebase that needs it.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_covers_five_chars() {
        assert_eq!(
            escape_html("<script>alert(\"&x'y\")</script>"),
            "&lt;script&gt;alert(&quot;&amp;x&#39;y&quot;)&lt;/script&gt;"
        );
    }

    #[test]
    fn text_template_includes_invite_url_and_message() {
        let body = format_invite_text(
            "Alice",
            "forecast",
            "edit",
            "https://example.com/invites/abc",
            Some("Would love your take on this."),
            "2026-08-01",
        );
        assert!(body.contains("Alice invited you to edit a forecast."));
        assert!(body.contains("https://example.com/invites/abc"));
        assert!(body.contains("Would love your take on this."));
        assert!(body.contains("expires on 2026-08-01"));
    }

    #[test]
    fn text_template_omits_message_block_when_empty() {
        let body = format_invite_text(
            "Alice",
            "forecast",
            "view",
            "https://example.com/invites/abc",
            None,
            "2026-08-01",
        );
        assert!(!body.contains("Message:"));
    }

    #[test]
    fn html_template_escapes_inviter_name() {
        let html = format_invite_html(
            "<img src=x>",
            "forecast",
            "edit",
            "https://example.com/invites/abc",
            None,
            "2026-08-01",
        );
        assert!(html.contains("&lt;img src=x&gt;"));
        assert!(!html.contains("<img src=x>"));
    }

    #[test]
    fn invite_url_strips_trailing_slash_on_base() {
        let cfg = EmailConfig {
            api_key: String::new(),
            from: String::new(),
            app_base_url: "https://example.com/".into(),
            client: reqwest::Client::new(),
        };
        assert_eq!(cfg.invite_url("abc"), "https://example.com/invites/abc");
    }

    #[test]
    fn is_configured_true_only_when_key_set() {
        let cfg_no = EmailConfig {
            api_key: String::new(),
            from: String::new(),
            app_base_url: String::new(),
            client: reqwest::Client::new(),
        };
        let cfg_yes = EmailConfig {
            api_key: "re_x".into(),
            from: String::new(),
            app_base_url: String::new(),
            client: reqwest::Client::new(),
        };
        assert!(!cfg_no.is_configured());
        assert!(cfg_yes.is_configured());
    }
}
