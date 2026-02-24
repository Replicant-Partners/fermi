// custom-sw.js — Rabble push notification service worker
//
// Wraps Flutter's service worker and adds Web Push notification handling.
// The push pattern is "tickle + fetch": the server sends a minimal push
// to wake this worker, which then fetches the latest notifications from
// the API and displays them as native notifications.
//
// Registered from index.html instead of the default flutter_service_worker.js.

// Import Flutter's service worker for caching/offline support
importScripts("flutter_service_worker.js");

// ═══════════════════════════════════════════════════════════════════
// PUSH EVENT — fired when server sends a push notification
// ═══════════════════════════════════════════════════════════════════

self.addEventListener("push", function (event) {
  console.log("[SW] Push received", event.data ? "with data" : "empty tickle");

  // Always show a notification — even on empty tickle pushes.
  // Don't try to fetch from API (auth tokens not available in SW context).
  let title = "Rabble";
  let body = "You have new activity — tap to check";
  let tag = "rabble-notification";
  let type = "general";

  // Try to parse payload if present
  if (event.data) {
    try {
      const data = event.data.json();
      if (data.title) title = data.title;
      if (data.body) body = data.body;
      if (data.tag) tag = data.tag;
      if (data.type) type = data.type;
    } catch (e) {
      // Non-JSON payload — try as text
      try {
        const text = event.data.text();
        if (text && text.length > 0 && text.length < 200) {
          body = text;
        }
      } catch (e2) {
        // Empty payload — use defaults
      }
    }
  }

  const options = {
    body: body,
    icon: "/icons/Icon-192.png",
    badge: "/icons/Icon-maskable-192.png",
    tag: tag + "-" + Date.now(),
    renotify: true,
    vibrate: [100, 50, 100],
    data: {
      type: type,
      url: "/",
      timestamp: Date.now(),
    },
    actions: _getActionsForType(type),
  };

  event.waitUntil(self.registration.showNotification(title, options));
});

// ═══════════════════════════════════════════════════════════════════
// NOTIFICATION CLICK — navigate to relevant screen
// ═══════════════════════════════════════════════════════════════════

self.addEventListener("notificationclick", function (event) {
  console.log(
    "[SW] Notification click:",
    event.action,
    event.notification.data,
  );

  event.notification.close();

  const data = event.notification.data || {};
  let targetUrl = "/";

  // Route based on notification type
  switch (data.type) {
    case "friendship_request":
    case "friendship_accepted":
      // Navigate to notifications screen (has Accept/Decline)
      targetUrl = "/#/notifications";
      break;
    case "creature_invite":
    case "creature_invite_accepted":
      targetUrl = "/#/notifications";
      break;
    case "rabble_join":
    case "rabble_start":
    case "rabble_end":
    case "rabble_invite":
      targetUrl = data.url || "/#/rabbles";
      break;
    case "creature_gift":
      targetUrl = "/#/creatures";
      break;
    default:
      targetUrl = data.url || "/";
  }

  // Handle action buttons (if supported by browser)
  if (event.action === "accept") {
    // Accept action — open notifications screen where Accept button is
    targetUrl = "/#/notifications";
  } else if (event.action === "view") {
    targetUrl = data.url || "/";
  }

  // Focus existing window or open new one
  event.waitUntil(
    clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then(function (clientList) {
        // Try to focus an existing window
        for (const client of clientList) {
          if (client.url.includes(self.location.origin) && "focus" in client) {
            client.focus();
            // Navigate within the app
            client.postMessage({
              type: "NOTIFICATION_CLICK",
              url: targetUrl,
              notificationType: data.type,
            });
            return;
          }
        }
        // No existing window — open new one
        if (clients.openWindow) {
          return clients.openWindow(targetUrl);
        }
      }),
  );
});

// ═══════════════════════════════════════════════════════════════════
// NOTIFICATION CLOSE — analytics (optional)
// ═══════════════════════════════════════════════════════════════════

self.addEventListener("notificationclose", function (event) {
  console.log("[SW] Notification dismissed:", event.notification.tag);
});

// ═══════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════

/**
 * Fetch the latest unread notification from the API and display it.
 * Used when the push is a "tickle" (empty payload).
 */
async function _fetchAndShowLatestNotification(fallbackOptions) {
  try {
    // Get auth token from IndexedDB or cache (if available)
    const token = await _getAuthToken();
    if (!token) {
      return self.registration.showNotification("Rabble", fallbackOptions);
    }

    const response = await fetch("/api/notifications?limit=1&unread=true", {
      headers: {
        Authorization: "Bearer " + token,
        "Content-Type": "application/json",
      },
    });

    if (!response.ok) {
      return self.registration.showNotification("Rabble", fallbackOptions);
    }

    const data = await response.json();
    const notifications = data.notifications || [];

    if (notifications.length === 0) {
      // No unread notifications — show generic
      return self.registration.showNotification("Rabble", {
        ...fallbackOptions,
        body: "Check your rabbles for new activity",
      });
    }

    const notif = notifications[0];
    const type = notif.type || "general";
    const meta = notif.metadata || {};

    return self.registration.showNotification(notif.title || "Rabble", {
      body: notif.message || "",
      icon: "/icons/Icon-192.png",
      badge: "/icons/Icon-maskable-192.png",
      tag: type + "-" + (notif.id || Date.now()),
      renotify: true,
      vibrate: [100, 50, 100],
      data: {
        type: type,
        url: _getUrlForType(type, meta),
        notificationId: notif.id,
      },
      actions: _getActionsForType(type),
    });
  } catch (e) {
    console.error("[SW] Failed to fetch notifications:", e);
    return self.registration.showNotification("Rabble", fallbackOptions);
  }
}

/**
 * Get notification actions based on type.
 * Not all browsers support actions — they're progressive enhancement.
 */
function _getActionsForType(type) {
  switch (type) {
    case "friendship_request":
      return [
        { action: "accept", title: "❤️ Accept", icon: "/icons/Icon-192.png" },
        { action: "view", title: "👀 View", icon: "/icons/Icon-192.png" },
      ];
    case "creature_invite":
      return [
        { action: "accept", title: "✅ Accept", icon: "/icons/Icon-192.png" },
        { action: "view", title: "👀 View", icon: "/icons/Icon-192.png" },
      ];
    case "rabble_join":
    case "rabble_start":
      return [
        {
          action: "view",
          title: "👀 Open Rabble",
          icon: "/icons/Icon-192.png",
        },
      ];
    default:
      return [];
  }
}

/**
 * Get the target URL for a notification type + metadata.
 */
function _getUrlForType(type, meta) {
  switch (type) {
    case "friendship_request":
    case "friendship_accepted":
    case "creature_invite":
    case "creature_invite_accepted":
      return "/#/notifications";
    case "rabble_join":
    case "rabble_start":
    case "rabble_end":
    case "rabble_invite":
      if (meta.swarm_id) return "/#/rabble/" + meta.swarm_id;
      return "/#/rabbles";
    case "creature_gift":
      if (meta.creature_id) return "/#/creature/" + meta.creature_id;
      return "/#/creatures";
    default:
      return "/";
  }
}

/**
 * Try to get the auth token from various storage mechanisms.
 * Service workers can't access localStorage, so we try:
 *   1. IndexedDB (shared_preferences stores here on web)
 *   2. Cache API (if token was cached)
 */
async function _getAuthToken() {
  try {
    // Try IndexedDB — Flutter's shared_preferences uses this on web
    const db = await new Promise((resolve, reject) => {
      const request = indexedDB.open("FlutterSharedPreferences", 1);
      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);
      // If the DB doesn't exist, just resolve null
      request.onupgradeneeded = () => {
        request.result.close();
        resolve(null);
      };
    });

    if (!db) return null;

    const tx = db.transaction("PreferencesStore", "readonly");
    const store = tx.objectStore("PreferencesStore");

    const token = await new Promise((resolve, reject) => {
      const request = store.get("flutter.auth_token");
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result);
    });

    db.close();

    if (token && typeof token === "string") {
      return token;
    }

    return null;
  } catch (e) {
    console.warn("[SW] Could not read auth token from IndexedDB:", e);
    return null;
  }
}

// ═══════════════════════════════════════════════════════════════════
// LIFECYCLE — standard service worker events
// ═══════════════════════════════════════════════════════════════════

self.addEventListener("install", function (event) {
  console.log("[SW] Custom service worker installed (push-enabled)");
  // Skip waiting to activate immediately
  self.skipWaiting();
});

self.addEventListener("activate", function (event) {
  console.log("[SW] Custom service worker activated");
  // Claim all clients immediately
  event.waitUntil(clients.claim());
});

// Listen for messages from the Flutter app (e.g. auth token updates)
self.addEventListener("message", function (event) {
  if (event.data && event.data.type === "AUTH_TOKEN_UPDATE") {
    console.log("[SW] Auth token updated");
    // Token is stored in IndexedDB by Flutter — we'll read it on next push
  }
});
