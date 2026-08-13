/**
 * Admin "view as user" banner.
 *
 * Included on every page. Calls /api/auth/me once; if the response
 * carries an `impersonation` block, pins an unmissable bar to the top of
 * the viewport naming the account being viewed and offering an exit.
 *
 * Why a banner is load-bearing rather than cosmetic: while impersonating,
 * every surface renders the *target's* data and identity. Without a
 * persistent, high-contrast marker there is nothing on screen to
 * distinguish "this is a user's broken workspace" from "this is my own",
 * which is how an operator ends up mistaking one for the other.
 *
 * Fails silent: an auth error or an anonymous page must never break
 * rendering.
 */
(function () {
  "use strict";

  var BANNER_ID = "abw-impersonation-banner";
  var EXIT_ENDPOINT = "/api/admin/impersonate/end";

  function el(tag, style, text) {
    var node = document.createElement(tag);
    if (style) node.setAttribute("style", style);
    if (text) node.textContent = text;
    return node;
  }

  function render(imp) {
    if (document.getElementById(BANNER_ID)) return;

    var bar = el(
      "div",
      [
        "position:fixed",
        "top:0",
        "left:0",
        "right:0",
        "z-index:2147483647",
        "background:repeating-linear-gradient(45deg,#8f3f71,#8f3f71 12px,#7c3663 12px,#7c3663 24px)",
        "color:#fbf1c7",
        "font-family:ui-monospace,'Cascadia Code','Source Code Pro',Menlo,Consolas,monospace",
        "font-size:13px",
        "line-height:1.4",
        "padding:8px 14px",
        "display:flex",
        "align-items:center",
        "gap:12px",
        "flex-wrap:wrap",
        "box-shadow:0 2px 10px rgba(0,0,0,.45)",
      ].join(";"),
    );
    bar.id = BANNER_ID;

    var label = el("span", "font-weight:700;letter-spacing:.04em;");
    label.textContent = "👁  VIEWING AS";

    var who = el(
      "span",
      "background:rgba(0,0,0,.35);padding:2px 8px;border-radius:3px;font-weight:700;",
      imp.viewing_as || "unknown user",
    );

    var mode = el(
      "span",
      "opacity:.9;",
      imp.mode === "read_only"
        ? "read-only — writes, credentials and billing are blocked"
        : "assist mode — mutations are permitted and logged",
    );

    var spacer = el("span", "flex:1 1 auto;");

    var exit = el(
      "button",
      [
        "background:#fbf1c7",
        "color:#1d2021",
        "border:none",
        "border-radius:3px",
        "padding:4px 12px",
        "font:inherit",
        "font-weight:700",
        "cursor:pointer",
      ].join(";"),
      "Exit",
    );
    exit.addEventListener("click", function () {
      exit.disabled = true;
      exit.textContent = "Exiting…";
      fetch(EXIT_ENDPOINT, {
        method: "POST",
        credentials: "same-origin",
      })
        .catch(function () {
          /* reload anyway: the cookie may still have been cleared */
        })
        .then(function () {
          window.location.reload();
        });
    });

    bar.appendChild(label);
    bar.appendChild(who);
    bar.appendChild(mode);
    bar.appendChild(spacer);
    bar.appendChild(exit);

    document.body.appendChild(bar);

    // Push the page down so the banner never covers a fixed header.
    var pad = bar.offsetHeight || 36;
    document.body.style.paddingTop = pad + "px";
  }

  function init() {
    fetch("/api/auth/me", { credentials: "same-origin" })
      .then(function (r) {
        return r.ok ? r.json() : null;
      })
      .then(function (me) {
        if (me && me.impersonation && me.impersonation.active) {
          render(me.impersonation);
        }
      })
      .catch(function () {
        /* anonymous or offline — nothing to show */
      });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
