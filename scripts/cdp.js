// A minimal browser driver, straight over the Chrome DevTools Protocol.
//
// ## Why not Playwright
//
// Playwright is the obvious choice and was the plan. It costs a `package.json`
// in a repo that has none, a `node_modules`, and a ~150MB browser download
// that has to be cached in CI. What it buys over the ~150 lines below is
// selectors, auto-waiting and cross-browser support — none of which the checks
// here need. They load two pages and read the console.
//
// Chrome is already installed, Node 24 has a global `WebSocket`, and CDP is a
// stable published protocol. So this has no dependencies at all: `node
// scripts/check_pages_headless.js` works on a clean checkout.
//
// If the checks ever grow to need real interaction — typing, drag, multiple
// browsers — this should be thrown away for Playwright rather than grown. It
// is deliberately the smallest thing that can answer "did the page load, did
// it style itself, and did anything throw".
//
// ## What a driver like this is FOR
//
// Three UI bugs in this repo reached a person's screen and were found by
// screenshot: tabs paired to panels by index, a standalone page missing its
// stylesheets, and a backtick inside an HTML comment inside a template literal
// that ended the string and blanked the page. A DOM stub executes JavaScript
// but has no layout and no stylesheets, so it cannot see any of them. This
// can.

const { spawn } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

const CHROME_CANDIDATES = [
  process.env.CHROME_PATH,
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
  "/opt/google/chrome/chrome",
  "/snap/bin/chromium",
].filter(Boolean);

function findChrome() {
  for (const c of CHROME_CANDIDATES) {
    try {
      fs.accessSync(c, fs.constants.X_OK);
      return c;
    } catch {}
  }
  return null;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function launch() {
  const bin = findChrome();
  if (!bin) throw new Error("NO_CHROME");

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "fermi-cdp-"));
  const proc = spawn(
    bin,
    [
      // `--headless=new` is the real renderer; the old headless was a separate
      // implementation and could differ from what a person sees, which would
      // defeat the point of using a browser at all.
      "--headless=new",
      "--remote-debugging-port=0",
      "--user-data-dir=" + dir,
      // Required in containers and CI. This browser only ever loads a local
      // fixture server, so the sandbox buys nothing here.
      "--no-sandbox",
      "--disable-gpu",
      "--disable-dev-shm-usage",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-extensions",
      // Off the network entirely. A check that silently depended on a CDN
      // would pass on a laptop and fail in CI, or worse, the reverse.
      "--disable-background-networking",
      "--disable-component-update",
      "--window-size=1400,1000",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );

  const stderr = [];
  proc.stderr.on("data", (d) => stderr.push(String(d)));

  // Chrome writes the port it actually chose here once the debugger is up.
  const portFile = path.join(dir, "DevToolsActivePort");
  let port = null;
  for (let i = 0; i < 200; i++) {
    if (proc.exitCode !== null)
      throw new Error(
        "chrome exited with " + proc.exitCode + ":\n" + stderr.join(""),
      );
    try {
      const txt = fs.readFileSync(portFile, "utf8").split("\n");
      if (txt[0] && txt[0].trim()) {
        port = Number(txt[0].trim());
        break;
      }
    } catch {}
    await sleep(50);
  }
  if (!port)
    throw new Error(
      "chrome never reported a debugging port:\n" + stderr.join(""),
    );

  const info = await (await fetch("http://127.0.0.1:" + port + "/json/version")).json();

  return {
    port,
    wsUrl: info.webSocketDebuggerUrl,
    async close() {
      try {
        proc.kill("SIGTERM");
      } catch {}
      // Give it a moment to unlink its lock files before the dir goes.
      await sleep(150);
      try {
        proc.kill("SIGKILL");
      } catch {}
      try {
        fs.rmSync(dir, { recursive: true, force: true });
      } catch {}
    },
  };
}

// One WebSocket to the browser, with `flatten: true` sessions so a page's
// messages ride the same socket tagged by `sessionId`. Simpler than a socket
// per target and it is the modern shape of the protocol.
async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = (e) => rej(new Error("cdp socket failed: " + e.message));
  });

  let nextId = 1;
  const pending = new Map();
  const listeners = [];

  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(msg.method + ": " + msg.error.message));
      else resolve(msg.result);
      return;
    }
    for (const fn of listeners) fn(msg);
  };

  const send = (method, params, sessionId) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params: params || {}, sessionId }));
    });

  return {
    send,
    on: (fn) => listeners.push(fn),
    close: () => ws.close(),
  };
}

/// Open a tab, instrument it, and return a handle.
///
/// Everything the checks care about is collected as it happens rather than
/// polled afterwards: a console error that fires during load is gone by the
/// time anyone asks the page about it.
async function openPage(cdp) {
  const { targetId } = await cdp.send("Target.createTarget", {
    url: "about:blank",
  });
  const { sessionId } = await cdp.send("Target.attachToTarget", {
    targetId,
    flatten: true,
  });

  const consoleErrors = [];
  const exceptions = [];
  const requests = new Map(); // requestId -> url
  const responses = []; // {url, status, type}
  const failures = []; // {url, error}
  let inFlight = 0;

  cdp.on((msg) => {
    if (msg.sessionId !== sessionId) return;
    switch (msg.method) {
      case "Runtime.consoleAPICalled":
        if (msg.params.type === "error" || msg.params.type === "assert")
          consoleErrors.push(
            (msg.params.args || [])
              .map((a) =>
                a.value !== undefined
                  ? String(a.value)
                  : a.description || a.type,
              )
              .join(" "),
          );
        break;
      case "Runtime.exceptionThrown": {
        const d = msg.params.exceptionDetails || {};
        exceptions.push(
          (d.exception && (d.exception.description || d.exception.value)) ||
            d.text ||
            "unknown exception",
        );
        break;
      }
      case "Network.requestWillBeSent":
        requests.set(msg.params.requestId, msg.params.request.url);
        inFlight++;
        break;
      case "Network.responseReceived":
        responses.push({
          url: msg.params.response.url,
          status: msg.params.response.status,
          type: msg.params.type,
        });
        break;
      case "Network.loadingFinished":
        inFlight--;
        break;
      case "Network.loadingFailed":
        inFlight--;
        failures.push({
          url: requests.get(msg.params.requestId) || "?",
          error: msg.params.errorText,
        });
        break;
    }
  });

  await cdp.send("Runtime.enable", {}, sessionId);
  await cdp.send("Network.enable", {}, sessionId);
  await cdp.send("Page.enable", {}, sessionId);

  const page = {
    consoleErrors,
    exceptions,
    responses,
    failures,

    async goto(url, { settleMs = 900, timeoutMs = 20000 } = {}) {
      const loaded = new Promise((resolve) => {
        const off = (msg) => {
          if (msg.sessionId === sessionId && msg.method === "Page.loadEventFired")
            resolve();
        };
        cdp.on(off);
      });
      await cdp.send("Page.navigate", { url }, sessionId);
      await Promise.race([loaded, sleep(timeoutMs)]);
      // The pages here render from XHR after load, so `load` is the start of
      // the interesting part rather than the end. Wait for the requests to
      // drain, then a beat for the renders they trigger.
      const until = Date.now() + timeoutMs;
      while (inFlight > 0 && Date.now() < until) await sleep(50);
      await sleep(settleMs);
    },

    /// Evaluate in the page and return the value. Throws what the page threw,
    /// so a broken assertion expression is not silently a `false`.
    async eval(fn, ...args) {
      const expr =
        "(" + fn.toString() + ")(" + args.map((a) => JSON.stringify(a)).join(",") + ")";
      const r = await cdp.send(
        "Runtime.evaluate",
        { expression: expr, returnByValue: true, awaitPromise: true },
        sessionId,
      );
      if (r.exceptionDetails)
        throw new Error(
          "page threw during eval: " +
            (r.exceptionDetails.exception
              ? r.exceptionDetails.exception.description
              : r.exceptionDetails.text),
        );
      return r.result.value;
    },

    async close() {
      await cdp.send("Target.closeTarget", { targetId });
    },
  };
  return page;
}

module.exports = { findChrome, launch, connect, openPage, sleep };
