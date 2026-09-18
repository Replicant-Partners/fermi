# Deploy trigger 2026-09-16T11:58:00Z

The deployed image is stuck at `63d74096` ("dpp_companion, and the shelf it
lives on"). Six commits pushed after it — over roughly ninety minutes — were
never built, so the DPP Studio App on the live site is serving a UI that
predates both the UI-scale control and the carbon statement panel.

How we know it is the image and not the browser: the served
`static/adaptogen-lab/index.html` is 174,500 bytes, byte-for-byte the size of
that file at `63d74096`, against 208,885 bytes at `main`. Its `last-modified`
header is `2026-09-16 10:20:36 GMT`, which is `12:20:36 +0200` — `63d74096`'s
commit timestamp to the second. The markers `uiscale-label` and the carbon
panel are absent from the served bytes, so no amount of cache-busting on the
client can produce them.

The undeployed range is not a broken build. `e9b49f2f` was checked out into a
clean worktree, the three desktop-crate `sed` strips from `Dockerfile:27-29`
were applied, and `cargo check --release --bin api-server` finished with
warnings only. `Dockerfile:41` runs no tests, so nothing else in the image
build can reject it.

That leaves the GitHub -> Railway auto-deploy as the thing that stopped. This
file exists to give it a commit it cannot ignore.

After deploy, the check is on the artifact rather than the dashboard:

    curl -sI https://agent-bestiary.world/static/adaptogen-lab/index.html \
      | grep -i last-modified
    curl -s https://agent-bestiary.world/static/adaptogen-lab/index.html \
      | grep -c uiscale-label

`last-modified` should move to this deploy and the second command should
return `2`, not `0`. If `last-modified` does not move, auto-deploy is
genuinely disconnected and the Railway build logs are the next place to look —
a queued or errored build there would explain a push that produces no image.

---

## 2026-09-18 — `3c160fb4`, accept-and-detach

`b8d4ed11` deployed. `3c160fb4` did not, and the served artifact said so before
the dashboard would have:

    content-length: 231941   (served)
    242859                   (static/adaptogen-lab/index.html on disk)
    last-modified: Fri, 18 Sep 2026 11:47:32 GMT   (the b8d4ed11 build)
    grep -c carbonArtefactPath  ->  0

Twelve minutes after the push, so this is the §6 case again rather than a slow
build. Nudging.

`3c160fb4` is the one commit where a stale image is actively misleading rather
than merely old. It makes `POST /actions/calculate_carbon` return `202` with an
`action_id` and detach the work, and it rewrites the client to poll the action
log instead of holding the connection. Half-deployed in either direction looks
like a product defect:

  * new page against old server — the client reads `data.action_id` from a
    synchronous 200 that does not carry one, and reports "accepted a run but
    returned no action_id" for a run that in fact completed;
  * old page against new server — the client treats a `202` as a finished run,
    finds no `statement` in it, and renders an empty panel while the real run
    is still going and will complete unobserved.

Both halves are in this one commit, so the only thing to avoid is serving the
page from a different image than the API. After deploy:

    curl -s https://agent-bestiary-production.up.railway.app/api/health \
      | python3 -c "import sys,json; print(json.load(sys.stdin)['commit'])"
    curl -s https://agent-bestiary-production.up.railway.app/static/adaptogen-lab/index.html \
      | grep -c carbonArtefactPath

The first should read `3c160fb4…`, the second `3`. `0` on the second with the
right commit on the first would mean the static directory and the binary came
from different builds, which is worth knowing on its own.
