/* Auth UI — populates #user-area with login/logout controls.
   Call initAuth() after DOM is ready, or include this script at end of body.
   Optionally pass a container id (defaults to "user-area"). */

function initAuth(containerId) {
  var area = document.getElementById(containerId || "user-area");
  if (!area) return;

  fetch("/api/auth/me")
    .then(function (res) {
      if (res.ok) return res.json();
      throw new Error("not authenticated");
    })
    .then(function (user) {
      var name = user.display_name || user.email || "User";
      area.innerHTML =
        '<span class="auth-user">' + name + "</span>" +
        "<button class=\"auth-logout\" onclick=\"fetch('/auth/logout',{method:'POST'}).then(function(){location.reload()})\">sign out</button>";
    })
    .catch(function () {
      area.innerHTML =
        '<a href="/auth/google" class="auth-btn">Google</a>' +
        '<a href="/auth/github" class="auth-btn">GitHub</a>' +
        (window.ethereum
          ? '<button class="auth-btn" onclick="connectWallet()" style="cursor:pointer">Wallet</button>'
          : "");
    });
}

/* SIWE wallet connect */
async function connectWallet() {
  if (!window.ethereum) return;
  try {
    var accounts = await window.ethereum.request({ method: "eth_requestAccounts" });
    var address = accounts[0];
    var challengeRes = await fetch("/auth/siwe/challenge", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ address: address }),
    });
    if (!challengeRes.ok) { alert("Failed to get challenge"); return; }
    var data = await challengeRes.json();
    var signature = await window.ethereum.request({
      method: "personal_sign",
      params: [data.message, address],
    });
    var verifyRes = await fetch("/auth/siwe/verify", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message: data.message, signature: signature }),
    });
    if (verifyRes.ok) {
      window.location.href = "/dashboard";
    } else {
      alert("Wallet verification failed");
    }
  } catch (e) {
    console.error("Wallet connect error:", e);
  }
}

/* Auto-init if script loaded at end of body */
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", function () { initAuth(); });
} else {
  initAuth();
}
