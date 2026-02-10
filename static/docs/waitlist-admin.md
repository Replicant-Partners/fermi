# Waitlist & Invitation Management

This guide is for **admin users only**. It covers how the waitlist system works end-to-end: from a visitor entering their email on the landing page, through admin review, to adding them as a Google OAuth test user so they can sign in.

## How the Waitlist Works

1. A visitor lands on `agent-bestiary.world` and enters their email in the signup form.
2. The email is stored in the `waitlist` table with status `pending`.
3. An admin reviews the waitlist in the Admin Panel.
4. The admin copies the email(s) and adds them as test users in Google Cloud Console.
5. The admin marks the entries as `invited` to track who has been processed.
6. The invited user can now sign in with Google OAuth.

## Accessing the Admin Panel

Navigate to `/admin`. You must be signed in with an account that has the `admin` role. If your account doesn't have admin access, you'll see an "Access Denied" message.

## The Waitlist Tab

Click the **Waitlist** tab in the Admin Panel. You'll see:

- **Stat card** at the top showing `pending / total` signup count.
- **Search bar** to filter by email address.
- **Status filter** dropdown: All, Pending, or Invited.
- **Copy buttons** for bulk operations.
- **Add email form** for manual entries.
- **Table** with every waitlist entry.

### Table Columns

| Column | Description |
|--------|-------------|
| Checkbox | Select entries for bulk actions |
| Email | The signup email |
| Source | Where they signed up (`landing` = website form, `admin` = manually added) |
| Status | `pending` (waiting) or `invited` (processed) |
| Notes | Optional admin notes |
| Signed Up | When they submitted the form |
| Invited | When the admin marked them as invited |
| Actions | Per-row Invite and Delete buttons |

## Inviting Users: Step by Step

### 1. Review pending signups

Go to the Waitlist tab. Use the status filter to show only **Pending** entries.

### 2. Copy emails to clipboard

Click **Copy Pending Emails**. This copies all pending email addresses to your clipboard, one per line. A toast notification confirms the count.

Alternatively, select specific rows with checkboxes and use the selection to choose exactly who to invite.

### 3. Add as Google OAuth test users

Open the [Google Cloud Console](https://console.cloud.google.com/) for the Agent Bestiary project:

1. Go to **Google Auth Platform** (left sidebar, under APIs & Services).
2. Click **Audience**.
3. Scroll to the **Test users** section.
4. Click **Add users**.
5. Paste the email addresses (one per line or comma-separated both work).
6. Save.

Google allows up to **100 test users** while the app is in Testing mode. Once you publish the app to Production, the test user limit is removed.

### 4. Mark as invited

Back in the Admin Panel, select the entries you just added to Google and click **Mark Selected as Invited**. This updates their status and records the invitation timestamp.

You can also click the **Invite** button on individual rows.

### 5. Notify the user (manual for now)

Send the user an email or message letting them know they can sign in at `agent-bestiary.world` using their Google account.

## Adding Emails Manually

Use the form at the top of the Waitlist tab:

1. Type an email address in the input field.
2. Optionally add a note (e.g., "Met at conference", "Team member").
3. Click **Add**.

The entry is created with source `admin` and status `pending`. If the email already exists, the note is updated.

## Removing Entries

Click the red **Del** button on any row. You'll be asked to confirm. This permanently removes the entry from the waitlist.

## API Reference

All endpoints require admin authentication (the `abw_session` cookie with an admin-role JWT).

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/admin/waitlist` | List entries. Query params: `?search=`, `?status=pending\|invited` |
| `POST` | `/api/admin/waitlist` | Add an email. Body: `{ "email": "...", "notes": "..." }` |
| `POST` | `/api/admin/waitlist/invite` | Bulk invite. Body: `{ "emails": ["a@b.com", ...], "notes": "..." }` |
| `DELETE` | `/api/admin/waitlist/:entry_id` | Remove an entry by UUID |

### Example: List pending entries

```bash
curl -b cookies.txt "https://agent-bestiary.world/api/admin/waitlist?status=pending"
```

### Example: Bulk invite

```bash
curl -b cookies.txt -X POST \
  -H "Content-Type: application/json" \
  -d '{"emails": ["alice@example.com", "bob@example.com"]}' \
  "https://agent-bestiary.world/api/admin/waitlist/invite"
```

## Moving to Production

When you're ready to open the platform beyond 100 test users:

1. In Google Cloud Console, go to Google Auth Platform > Audience.
2. Click **Publish App** to move from Testing to Production.
3. If the app uses sensitive scopes, Google may require a verification review. For basic profile/email scopes, publishing is typically immediate.
4. Once published, any Google account holder can sign in — the test user list is no longer enforced.

The waitlist system remains useful even after publishing: you can still use it to track interest, manage invite waves, and control who gets early access at the application level.
