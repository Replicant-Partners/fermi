-- Product passport publications: a DPP released to people with no ABW account.
--
-- # What this is for
--
-- A passport is only a passport if someone who does not trust the bearer can
-- read it. Until this migration every read of a DPP went through workspace
-- membership, so the scannable code on the passport resolved only for people
-- who could already open the workspace — a passport its holders could read and
-- nobody else.
--
-- Publishing is an explicit act by a workspace admin, and what it publishes is
-- a SNAPSHOT, not a live view.
--
-- # Why a snapshot and not a live read
--
-- A release is a statement made at a moment. If the public page read the
-- workspace live, an operator editing a draft claim at 10:00 would be
-- publishing that draft at 10:00, and a regulator reading the page at 10:01
-- would see something nobody released. The snapshot is what the publisher saw
-- and chose to release, with the time they released it, and later edits stay
-- private until they release again.
--
-- It also makes the field allowlist auditable: the snapshot is the complete
-- public surface, stored, so "what did we make public on the 14th" has an
-- answer that is a row rather than a reconstruction.
--
-- # Why the token is stable across releases
--
-- The token is printed. It ends up in a QR on a label, a box, a shelf-edge
-- tag. Rotating it on every release would break every code already in the
-- world the first time a claim was corrected, which would teach operators not
-- to correct claims. So releasing again replaces the snapshot behind the same
-- token, and `release` counts how many times that has happened. Rotation
-- exists, as an explicit choice, for the case where a link reached someone it
-- should not have.
--
-- # Why the token is stored in plaintext, unlike ground_runs
--
-- ground_runs stores a SHA-256 because its token is a credential: it
-- authorises tool calls that spend money. This token authorises reading a
-- document its owner chose to make public, and the owner needs to be able to
-- copy the link again. It is a capability URL — an unlisted link — and is
-- generated with enough entropy (24 chars of [a-z0-9], ~124 bits) that it is
-- not enumerable, which an 8-char rabble join token would be on an endpoint
-- that needs no login.
--
-- # History
--
-- Every release is kept in dpp_publication_releases. The twin evolves, and the
-- record of what it said publicly at each point is the part a reader with a
-- complaint, or an auditor, actually needs.

CREATE TABLE IF NOT EXISTS public.dpp_publications (
    publication_id UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    -- One live public link per product. A second publish updates this row.
    workspace_id   UUID        NOT NULL UNIQUE REFERENCES public.teams(id) ON DELETE CASCADE,
    token          TEXT        NOT NULL UNIQUE,
    -- The current release, exactly as served to the public.
    snapshot       JSONB       NOT NULL,
    sections       TEXT[]      NOT NULL,
    release        INTEGER     NOT NULL DEFAULT 1,
    first_published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set by unpublish. A revoked link answers 410, not 404: the passport
    -- existed and was withdrawn, which a reader holding a printed code should
    -- be told rather than left to assume a typo.
    revoked_at     TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS public.dpp_publication_releases (
    publication_id UUID        NOT NULL REFERENCES public.dpp_publications(publication_id) ON DELETE CASCADE,
    release        INTEGER     NOT NULL,
    snapshot       JSONB       NOT NULL,
    sections       TEXT[]      NOT NULL,
    -- Audit: who released it. Not an ownership column — the release outlives
    -- any change in who administers the workspace.
    released_by    TEXT        NOT NULL,
    released_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (publication_id, release)
);

CREATE INDEX IF NOT EXISTS idx_dpp_publication_releases_time
    ON public.dpp_publication_releases (publication_id, released_at DESC);
