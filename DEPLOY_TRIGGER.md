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
