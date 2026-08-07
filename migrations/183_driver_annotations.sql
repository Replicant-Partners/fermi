-- ─────────────────────────────────────────────────────────────────────
-- 183 — driver annotations (Spec 32)
-- ─────────────────────────────────────────────────────────────────────
--
-- WHY
--
-- The operator's model: teams "coordinate on trajectories and research and
-- assumptions". Everything built so far coordinates on the first two —
-- provenance, history, the ops board. Nothing lets a teammate say the most
-- common thing there is to say about a forecast:
--
--     "your base rate for elo_current is wrong, here's why"
--
-- Today that conversation happens in Slack, or as a probability revision
-- with a `reason` string, or not at all. None of those attach to the thing
-- being disputed, so the objection is invisible to the next person who
-- opens the forecast — which is precisely when it matters.
--
-- WHY THE DRIVER, NOT THE FORECAST
--
-- A forecast-level comment thread would be easier and much less useful.
-- Disagreement in this product is almost never about the question; it is
-- about one input. `drivers` is where the assumptions live, and Spec 31's
-- git history already versions them. Anchoring an objection to
-- (forecast, driver) means:
--
--   * it renders next to the number it disputes;
--   * it survives a revision of some *other* driver;
--   * "which assumptions are contested" becomes a query, which the ops
--     board turns into coordination work.
--
-- WHERE DRIVERS ACTUALLY LIVE
--
-- `driver_name` is TEXT, not a foreign key, because drivers are not rows.
-- A driver is a `driver <name> { ... }` declaration inside the forecast's
-- FPL program (`fermi_forecasts.fpl_source`), which is what the executor,
-- the LSP and BayesOps all read — `bayesops_*.driver_name` is keyed exactly
-- the same way, by name, for the same reason.
--
-- Note that `fermi_forecasts.drivers` (JSONB) looks like the natural anchor
-- and is not: nothing populates it, and every row in production holds an
-- empty array. Do not key anything to it.
--
-- Since a name is not a reference, a driver can be renamed or removed out
-- from under an annotation. That is tolerated deliberately — see
-- `status = 'orphaned'` below — because the alternative (normalising an FPL
-- language construct into a table) is a far larger change than this feature
-- justifies.
--
-- STATUS, NOT DELETION
--
-- An annotation is a claim someone made. Resolving it should record what
-- happened, not erase it: 'accepted' (the driver changed as a result) and
-- 'declined' (considered, rejected) are different outcomes and the
-- difference is exactly the kind of reasoning a team wants to be able to
-- re-read. Only the author may hard-delete, for genuine mistakes.

CREATE TABLE IF NOT EXISTS public.driver_annotations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    forecast_id     TEXT NOT NULL
                        REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE,

    -- The disputed input. NULL means the annotation is about the forecast
    -- as a whole — allowed, because "the whole framing is wrong" is a real
    -- and useful thing to say, and forcing it onto an arbitrary driver
    -- would misfile it.
    driver_name     TEXT,

    author_id       TEXT NOT NULL,
    body            TEXT NOT NULL,

    -- 'challenge'  — this input is wrong (the load-bearing kind)
    -- 'question'   — I don't understand this
    -- 'note'       — context worth recording, no action implied
    kind            TEXT NOT NULL DEFAULT 'challenge'
                        CHECK (kind IN ('challenge', 'question', 'note')),

    -- 'open'      — awaiting a response
    -- 'accepted'  — acted on; the driver changed
    -- 'declined'  — considered and rejected
    -- 'orphaned'  — the driver it referenced no longer exists
    status          TEXT NOT NULL DEFAULT 'open'
                        CHECK (status IN ('open', 'accepted', 'declined', 'orphaned')),

    resolved_by     TEXT,
    resolved_at     TIMESTAMPTZ,
    resolution_note TEXT,

    -- The commit the annotation was written against (Spec 31). Lets the UI
    -- say "raised when the driver read 1780" even after it has moved, which
    -- is the difference between a comment that ages into nonsense and one
    -- that stays legible.
    at_commit       TEXT,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A resolution must record who and when, or "accepted" is unattributable
    -- — the same attribution gap Spec 26 existed to close, and there is no
    -- reason to reintroduce it in a new table.
    CONSTRAINT driver_annotations_resolution_complete
        CHECK ((status = 'open')
            OR (status = 'orphaned')
            OR (resolved_by IS NOT NULL AND resolved_at IS NOT NULL))
);

-- The dominant read: "every annotation on this forecast", rendered beside
-- the drivers. Ordered so open items surface first.
CREATE INDEX IF NOT EXISTS idx_driver_annotations_forecast
    ON public.driver_annotations(forecast_id, created_at DESC);

-- "Which assumptions on this team's surface are contested" — the ops-board
-- detector. Partial, because only open ones are coordination work.
CREATE INDEX IF NOT EXISTS idx_driver_annotations_open
    ON public.driver_annotations(forecast_id, driver_name)
    WHERE status = 'open';

-- Per-person contribution roll-up (Spec 26 §4.3 already counts revisions,
-- resolutions and curations; raising and resolving challenges is the same
-- class of contribution).
CREATE INDEX IF NOT EXISTS idx_driver_annotations_author
    ON public.driver_annotations(author_id, created_at DESC);

COMMENT ON TABLE public.driver_annotations IS
    'Spec 32: objections and questions anchored to a specific driver of a forecast — "your base rate is wrong". Anchored at (forecast_id, driver_name) rather than the forecast because disagreement here is almost never about the question, it is about one input. driver_name is TEXT with no FK because drivers are not rows: a driver is a declaration inside the forecast''s FPL program (fpl_source), keyed by name, exactly as bayesops_*.driver_name is. (fermi_forecasts.drivers JSONB is vestigial and empty on every row — do not key to it.) A rename can therefore orphan an annotation — tolerated via status=''orphaned'' rather than normalising an FPL language construct into a table, which would be a far larger change than this feature justifies. NULL driver_name = an annotation on the forecast as a whole.';

COMMENT ON COLUMN public.driver_annotations.at_commit IS
    'The Spec 31 git SHA the annotation was written against, so the UI can say "raised when this read 1780" after the value has moved. Nullable: a forecast may not be versioned yet.';

COMMENT ON COLUMN public.driver_annotations.status IS
    'open | accepted (acted on, the driver changed) | declined (considered, rejected) | orphaned (the program no longer declares that driver; reverts back to open if it reappears, e.g. after a Spec 31 revert). Resolutions are recorded rather than deleted — the difference between accepted and declined is exactly the reasoning a team wants to re-read later.';
