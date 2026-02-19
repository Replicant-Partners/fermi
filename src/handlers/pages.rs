//! HTML page-serving handlers.

use axum::{
    http::{header, StatusCode},
    response::Html,
};

// ─── Fallback (404) ────────────────────────────────────────────────

#[allow(dead_code)]
pub async fn fallback_404() -> (StatusCode, Html<String>) {
    let html = std::fs::read_to_string("templates/404.html")
        .unwrap_or_else(|_| "<h1>404 — Not Found</h1>".to_string());
    (StatusCode::NOT_FOUND, Html(html))
}

// ─── Page routes ───────────────────────────────────────────────────

/// Landing page — serves Flutter web app for rabble.world, ABW landing for everything else
pub async fn landing(headers: axum::http::HeaderMap) -> Html<String> {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    if host.starts_with("rabble.world") {
        // Serve Flutter web app if built, otherwise show coming soon
        if let Ok(content) = std::fs::read_to_string("static/rabble/index.html") {
            return Html(content);
        }
        return Html("<html><body style='background:#0D1B14;color:#F5F0E8;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0'><div style='text-align:center'><h1>Rabble</h1><p style='color:#7A9A84'>Coming soon</p></div></body></html>".to_string());
    }

    let html = match std::fs::read_to_string("templates/landing.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/landing.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading landing template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn aspiration() -> Html<String> {
    let html = match std::fs::read_to_string("templates/aspiration.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/aspiration.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading aspiration template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn catalogue() -> Html<String> {
    println!("Catalogue route called");
    let html = match std::fs::read_to_string("templates/index.html") {
        Ok(content) => {
            println!(
                "Successfully loaded templates/index.html ({} bytes)",
                content.len()
            );
            content
        }
        Err(e) => {
            eprintln!("Error loading templates/index.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn agent_detail() -> Html<String> {
    let html = match std::fs::read_to_string("templates/agent_detail.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/agent_detail.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn ontology_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/ontology.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/ontology.html: {}", e);
            format!(
                "<h1>Knowledge Graph</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

// ─── API routes ────────────────────────────────────────────────────

pub async fn projector_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/projector.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/projector.html: {}", e);
            format!(
                "<h1>Embedding Projector</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn dashboard_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/dashboard.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/dashboard.html: {}", e);
            format!("<h1>Dashboard</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn agent_create_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/agent_create.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/agent_create.html: {}", e);
            format!("<h1>Create Agent</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn workspace_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/workspace.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/workspace.html: {}", e);
            format!("<h1>Workspace</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

// ─── Settings page ─────────────────────────────────────────────────

pub async fn settings_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/settings.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/settings.html: {}", e);
            format!("<h1>Settings</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn docs_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/docs.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/docs.html: {}", e);
            format!("<h1>Documentation</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn marketplace_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/marketplace.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/marketplace.html: {}", e);
            format!("<h1>Marketplace</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn admin_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/admin.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/admin.html: {}", e);
            format!("<h1>Admin</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}
