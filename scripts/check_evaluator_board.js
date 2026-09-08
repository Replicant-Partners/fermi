#!/usr/bin/env node
//
// # The evaluator board, rendered in a real browser
//
// `/api/evaluators` shipped with no consumer at all. The six checks the
// platform runs on its own machinery — the only verdicts anywhere on the
// platform that carry a written remedy — were reachable through
// `/api/admin/schema-health` and rendered by nothing.
//
// The board now exists, and it has one property that no unit test can see and
// that every previous version of these screens got wrong:
//
//   **`unknown` must not render as a pass.**
//
// Three of the six evaluators are usually `inconclusive`, because most of the
// counters they read are process-local `AtomicU64`s that reset on restart. A
// surface that colours `unknown` green therefore reports a healthy platform on
// every fresh boot — the one moment it is least entitled to. `notice` shares
// the same reading and means something different again: reported, never
// asserted.
//
// So this drives Chrome over the real template, with the API stubbed, and
// asserts on the classes the rows actually carry. The markup being correct is
// not the question; what a reader is shown is.
//
// Run by `tests/evaluator_board.rs`, or by hand:
//
//     node scripts/check_evaluator_board.js
//
// Exits non-zero with a written reason for each failure.

const http = require("http");
const fs = require("fs");
const path = require("path");
const { launch, connect, openPage } = require("./cdp.js");

const ROOT = path.join(__dirname, "..");
const FAIL = [];
const ok = (cond, what) => {
  if (!cond) FAIL.push(what);
};

// ── The fixture ──────────────────────────────────────────────────────────
//
// One evaluator per token, plus a token the page has never heard of. The
// unrecognised one is not padding: the served vocabulary is a closed set that
// *may grow*, and a new token falling through to a benign default is exactly
// how "nothing has been watched" once came to display as "the system is idle"
// on every panel backed by a loop.
const EVALUATORS = [
  {
    id: "positive_control",
    asks: "Has any part of the machinery been demonstrated to work?",
    reading: "fault",
    token: "critical",
    detail: "nothing has been demonstrated to work",
    remedy: "Check the sweeper before the paths.",
    subjects: ["0 live contracts, 0 turning loops"],
    caveat: {
      subject: "positive_control",
      checked: "At least one liveness contract is passing.",
      does_not_show: "Anything about the other contracts or loops.",
    },
  },
  {
    id: "refused_writes",
    asks: "Is a declared write path being refused every time it is attempted?",
    reading: "unknown",
    token: "inconclusive",
    detail: "no instrumented write has been attempted since boot",
    remedy: null,
    subjects: [],
    caveat: {
      subject: "refused_writes",
      checked: "No instrumented sink has been attempted and refused every time.",
      does_not_show: "That any write has succeeded, or that any has been attempted at all.",
    },
  },
  {
    id: "gate_admitting_everything",
    asks: "Has a gate been asked, and refused nothing?",
    reading: "unknown",
    token: "notice",
    detail: "1 gate(s) have never refused anything",
    remedy: "Not necessarily a fault.",
    subjects: ["coherence (40 asked, 0 refused) \u2014 nothing warranted refusal"],
    caveat: {
      subject: "gate_admitting_everything",
      checked: "Which gates have been asked and have refused nothing.",
      does_not_show: "That those gates are broken.",
    },
  },
  {
    id: "loop_stalled_in_code",
    asks: "Is a feedback loop stopped by a fault rather than by an absence of work?",
    reading: "idle",
    token: "healthy",
    detail: "2 of 6 loop(s) turning; the rest are idle rather than broken",
    remedy: null,
    subjects: [],
    // The caveat the handoff asks for by name. This evaluator's own sentence
    // over-claims: four of the six are stopped with reasons classified
    // `unknown` precisely because no contract can say.
    caveat: {
      subject: "loop_stalled_in_code",
      checked: "No loop's first empty link is a code fault.",
      does_not_show:
        "That the remaining loops are idle rather than broken, which is what its own " +
        "Healthy detail says.",
    },
  },
  {
    id: "undocumented_silence",
    asks: "Is a declared write path silent with no recorded excuse?",
    reading: "fault",
    token: "warning",
    detail: "2 sink(s) silent with no reason",
    remedy: "Either the writer is broken or it is not deployed.",
    subjects: ["loop2.anomaly: no_trigger", "coordinator_observation"],
    caveat: null,
  },
  {
    id: "a_check_from_the_future",
    asks: "Does the page invent a colour for a token it has never seen?",
    reading: "unknown",
    token: "some_token_shipped_later",
    detail: "a token from a closed set that grew",
    remedy: null,
    subjects: [],
    caveat: null,
  },
];

const API = {
  "/api/evaluators": {
    tally: { total: 6, healthy: 1, findings: 2, notices: 1, inconclusive: 1 },
    evaluators: EVALUATORS,
    doors: [],
    vocabulary: {
      reading: ["idle", "fault", "unknown"],
      token: ["healthy", "critical", "warning", "notice", "inconclusive"],
    },
    contract: "`inconclusive` is NOT a pass.",
  },
  // Enough of the neighbouring surfaces for the page to boot. The gate board
  // is real-shaped because the evaluator pane links a subject only when its
  // leading identifier is a gate this board declares.
  "/api/gates": {
    tally: { total: 1, discriminating: 0, inverted: 0, never_refused: 1, unexercised: 0 },
    gates: [
      {
        id: "coherence",
        refuses: "an agent-wide correction the world model rejects",
        approved: 40,
        refused: 0,
        since: "boot",
        token: "admits_everything",
        reading: "unknown",
      },
    ],
    doors: [],
    caveats: [],
    contract: "",
  },
  "/api/loops": {
    tally: { total: 0, turning: 0, stalled_by_fault: 0, stalled_idle: 0, no_reading: 0, unreadable: 0 },
    loops: [],
    vocabulary: {},
    contract: "",
  },
  "/api/loops/actions": { actions: [] },
  "/api/gates/enforcement": { commands: [], discarded: [], ungoverned: [] },
  "/api/episodes/recent": { episodes: [] },
  "/api/auth/me": { user: null },
  "/api/notifications": { notifications: [] },
};

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css",
  ".js": "text/javascript",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
};

function serve() {
  const server = http.createServer((req, res) => {
    const p = new URL(req.url, "http://127.0.0.1").pathname;

    if (p.startsWith("/api/")) {
      const body = JSON.stringify(API[p] ?? {});
      res.writeHead(200, {
        "content-type": "application/json",
        "content-length": Buffer.byteLength(body),
      });
      return res.end(body);
    }

    // The three page routes all serve one template, exactly as `loops_view`
    // does. Serving the same bytes is not an approximation of production here:
    // `app_shell` reads the file off disk and returns it with no interpolation.
    let file = null;
    if (p.startsWith("/static/")) file = path.join(ROOT, p.slice(1));
    else if (["/loops", "/gates", "/evaluators"].includes(p))
      file = path.join(ROOT, "templates/loops.html");

    if (!file || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      res.writeHead(404, { "content-type": "text/plain" });
      return res.end("not found: " + p);
    }
    const buf = fs.readFileSync(file);
    res.writeHead(200, {
      "content-type": MIME[path.extname(file)] || "application/octet-stream",
      "content-length": buf.length,
    });
    res.end(buf);
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port })),
  );
}

async function main() {
  const { server, port } = await serve();
  const base = `http://127.0.0.1:${port}`;
  let chrome, cdp;
  try {
    chrome = await launch();
    cdp = await connect(chrome.wsUrl);
    const page = await openPage(cdp);

    // The path, not a click. `/evaluators` has to open on the evaluator board;
    // three routes served one page and all three opened on Loops, which made a
    // link to a tab a link to a hunt for a tab.
    await page.goto(`${base}/evaluators`);

    ok(
      page.exceptions.length === 0,
      "the page threw during load, so everything after the throw rendered nothing:\n      " +
        page.exceptions.join("\n      "),
    );
    ok(
      page.consoleErrors.length === 0,
      "console error(s):\n      " + page.consoleErrors.join("\n      "),
    );

    const seen = await page.eval(() => {
      const rows = Array.from(document.querySelectorAll(".ev")).map((el) => ({
        id: (el.querySelector(".ev-id") || {}).textContent?.trim(),
        // The row's own reading class — what a reader is actually shown.
        cls: Array.from(el.classList).filter((c) => c !== "ev"),
        token: (el.querySelector(".badge") || {}).textContent?.trim(),
        detail: (el.querySelector(".ev-detail") || {}).textContent || "",
        narrow: !!el.querySelector(".ev-narrow"),
        caveat: (el.querySelector(".ev-cav") || {}).textContent || "",
        subjects: Array.from(el.querySelectorAll(".ev-s")).map((s) => ({
          text: s.textContent.trim(),
          href: s.getAttribute("href"),
        })),
        remedy: (el.querySelector(".ev-rem") || {}).textContent || "",
      }));
      return {
        rows,
        activeTab: (document.querySelector(".tab.active") || {}).dataset?.tab,
        h1: (document.getElementById("page-h1") || {}).textContent?.trim(),
        buckets: Array.from(document.querySelectorAll(".tb")).map((b) => ({
          v: (b.querySelector(".tb-v") || {}).textContent?.trim(),
          l: (b.querySelector(".tb-l") || {}).textContent?.trim(),
        })),
        legend: Array.from(document.querySelectorAll(".ev-leg-row")).map((r) =>
          r.textContent.replace(/\s+/g, " ").trim(),
        ),
        doors: (document.querySelector(".nodoors") || {}).textContent || "",
      };
    });

    ok(
      seen.activeTab === "evaluators",
      `/evaluators opened the "${seen.activeTab}" tab. A route that lands on a different ` +
        `board than its name is a link a reader then has to search from.`,
    );
    ok(
      seen.h1 === "Evaluators",
      `the heading read "${seen.h1}" over the evaluator board. A page headed ` +
        `"Loops & Gates" showing a different surface is a small lie of the kind ` +
        `these screens exist to stop.`,
    );

    // 1. Every declared evaluator is drawn. A board that drops the checks it
    //    cannot report on looks shorter and healthier than it is.
    ok(
      seen.rows.length === EVALUATORS.length,
      `${seen.rows.length} of ${EVALUATORS.length} evaluators rendered. The missing ones ` +
        `are: ${EVALUATORS.map((e) => e.id)
          .filter((id) => !seen.rows.some((r) => r.id === id))
          .join(", ")}`,
    );

    // 2. THE property. `unknown` is not a pass and not a failure.
    for (const tok of ["inconclusive", "notice", "some_token_shipped_later"]) {
      const row = seen.rows.find((r) => (r.token || "").replace(/ /g, "_") === tok);
      if (!row) {
        FAIL.push(`no row rendered for token \`${tok}\``);
        continue;
      }
      ok(
        row.cls.includes("unknown"),
        `\`${tok}\` rendered as [${row.cls.join(", ")}] rather than \`unknown\`. Three of ` +
          `six evaluators are usually inconclusive, so a board that colours this as a ` +
          `pass reports a healthy platform on every fresh boot.`,
      );
      ok(
        !row.cls.includes("idle") && !row.cls.includes("fault"),
        `\`${tok}\` borrowed the ${row.cls.join("/")} colour. It is neither: it is the ` +
          `state where the platform is saying it does not know.`,
      );
      ok(
        !row.narrow,
        `\`${tok}\` was flagged "narrower than it reads", which belongs to a PASSING ` +
          `verdict that carries a caveat. On a non-pass it reads as a downgrade of ` +
          `something that was never a pass.`,
      );
    }

    // 3. A finding is a finding, or the board has no signal at all.
    for (const tok of ["critical", "warning"]) {
      const row = seen.rows.find((r) => r.token === tok);
      ok(row && row.cls.includes("fault"), `\`${tok}\` did not render as a fault`);
    }

    // 4. Every caveat reaches the reader. `does_not_show` is what a green tick
    //    fails to establish, and a tick rendered without it is the kind of lie
    //    that is very hard to notice.
    for (const e of EVALUATORS.filter((x) => x.caveat)) {
      const row = seen.rows.find((r) => r.id === e.id);
      ok(
        row && row.caveat.includes(e.caveat.does_not_show.slice(0, 40)),
        `\`${e.id}\` rendered no \`does_not_show\`. Every check the platform runs is ` +
          `narrower than the claim it serves, and this is the sentence that says so.`,
      );
    }

    // 5. The one the handoff asks for by name. `loop_stalled_in_code` returns
    //    Healthy saying "the rest are idle rather than broken" about four loops
    //    classified `unknown` precisely because no contract can say. The
    //    evaluator's own sentence is the thing that is wrong, so a pass with a
    //    caveat must be visibly qualified where the pass is shown.
    const stalled = seen.rows.find((r) => r.id === "loop_stalled_in_code");
    ok(
      stalled && stalled.narrow,
      "`loop_stalled_in_code` reads `healthy` and carries a caveat saying its own " +
        "detail over-claims, and the row did not mark it as narrower than it reads. " +
        "The caveat below a green row that nothing points at is a caveat nobody reads.",
    );

    // 6. The buckets partition and are never collapsed. "1 of 6 passing" invites
    //    a reader to conclude five are broken; "0 findings" invites the
    //    opposite. Both are wrong.
    ok(
      seen.buckets.length === 4,
      `${seen.buckets.length} tally buckets rendered, expected 4 (passing, findings, ` +
        `notices, inconclusive). Collapsing them is how a notice becomes a finding ` +
        `and an inconclusive becomes a pass.`,
    );
    const labels = seen.buckets.map((b) => b.l);
    for (const want of ["notices", "inconclusive"]) {
      ok(
        labels.includes(want),
        `the tally has no \`${want}\` bucket, so those evaluators are folded into one ` +
          `that asserts more than they do. Buckets seen: ${labels.join(", ")}`,
      );
    }

    // 7. Explain once. The reason belongs to the state, in one legend keyed by
    //    the token the rows print — not repeated on every row.
    ok(
      seen.legend.length === 5,
      `the legend has ${seen.legend.length} entries, expected 5 tokens over 3 readings`,
    );
    ok(
      seen.legend.some((l) => /inconclusive/.test(l) && /not a pass/i.test(l)),
      "the legend does not say that `inconclusive` is not a pass, which is the single " +
        "thing this board exists to communicate",
    );

    // 8. Doors: empty, and for a better reason than the gates'. Rendered rather
    //    than hidden — the day someone wants an "acknowledge this finding"
    //    button, the argument for it has to be written down.
    ok(
      /nothing to do here/i.test(seen.doors),
      "the empty door list was hidden rather than explained. An evaluator is a pure " +
        "function over a snapshot and a verdict a person could wave away would not be " +
        "worth computing — but a reader shown no control has to be told that, not left " +
        "to conclude the finding is unactionable.",
    );

    // 9. Act on the subject. A subject naming a gate the board declares becomes
    //    a link to that gate; one the page cannot resolve stays text, because a
    //    link built by guessing is worse than none.
    const admitting = seen.rows.find((r) => r.id === "gate_admitting_everything");
    ok(
      admitting && admitting.subjects.some((s) => s.href === "/gate/coherence"),
      "the `coherence` subject did not link to its gate. An evaluator has no door of " +
        "its own; the subject IS the control.",
    );
    const control = seen.rows.find((r) => r.id === "positive_control");
    ok(
      control && control.subjects.every((s) => s.href === null),
      "a subject that is a formatted sentence rather than an identifier was turned into " +
        "a link. Subject strings are written by the evaluator that raised them.",
    );

    // 10. A remedy is the whole reason these verdicts are worth surfacing.
    for (const e of EVALUATORS.filter((x) => x.remedy)) {
      const row = seen.rows.find((r) => r.id === e.id);
      ok(
        row && row.remedy.includes(e.remedy.slice(0, 30)),
        `\`${e.id}\` rendered no remedy. These are the only verdicts on the platform ` +
          `that carry one, which is why the board was worth building.`,
      );
    }
  } finally {
    try {
      if (cdp) cdp.close();
    } catch (_) {}
    try {
      if (chrome) await chrome.close();
    } catch (_) {}
    server.close();
  }

  if (FAIL.length) {
    console.error(`\n${FAIL.length} failure(s):\n`);
    FAIL.forEach((f) => console.error("  \u2716 " + f + "\n"));
    process.exit(1);
  }
  console.log(`OK  the evaluator board renders ${EVALUATORS.length} evaluators, and no`);
  console.log("    `unknown` reads as a pass.");
}

main().catch((e) => {
  console.error("harness error: " + (e && e.stack ? e.stack : e));
  process.exit(2);
});
