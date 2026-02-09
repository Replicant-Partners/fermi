// Shared API client — fetch wrapper with error handling and 401 redirect
const API = {
  async get(url) {
    return this._request(url, { method: "GET" });
  },

  async post(url, body) {
    return this._request(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: body != null ? JSON.stringify(body) : undefined,
    });
  },

  async put(url, body) {
    return this._request(url, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: body != null ? JSON.stringify(body) : undefined,
    });
  },

  async del(url) {
    return this._request(url, { method: "DELETE" });
  },

  async _request(url, opts) {
    const res = await fetch(url, opts);
    if (res.status === 401) {
      window.location.href = "/";
      throw new Error("Unauthorized");
    }
    if (!res.ok) {
      const msg = await res.text().catch(() => res.statusText);
      if (typeof Toast !== "undefined") {
        Toast.show(msg || "Request failed", "error");
      }
      throw new Error(msg || `HTTP ${res.status}`);
    }
    const ct = res.headers.get("content-type") || "";
    if (ct.includes("application/json")) {
      return res.json();
    }
    return res.text();
  },
};
