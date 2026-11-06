use crate::models::{BaseTemplate, PostDetailTemplate};
use actix_web::{error, Error, HttpRequest, HttpResponse};
use askama::Template;

// Allowlist of trusted Origins for Cross-Site Request Forgery (CSRF) protection
const ALLOWED_ORIGINS: [&str; 2] = ["http://127.0.0.1:8080", "http://localhost:8080"];

// Anti-CSRF (Cross-Site Request Forgery) Validation:
// Invoked on all state-mutating endpoints (POST / DELETE).
// Verifies the `Origin` and `Referer` headers against `ALLOWED_ORIGINS` and blocks (HTTP 403 Forbidden)
// any unauthorized cross-origin mutation requests initiated from third-party websites.
pub fn validate_origin(req: &HttpRequest) -> Result<(), Error> {
    let origin = req.headers().get("Origin").and_then(|v| v.to_str().ok());
    let referer = req.headers().get("Referer").and_then(|v| v.to_str().ok());

    let is_valid_origin = origin.is_some_and(|o| ALLOWED_ORIGINS.contains(&o));
    let is_valid_referer =
        referer.is_some_and(|r| ALLOWED_ORIGINS.iter().any(|allowed| r.starts_with(allowed)));

    if !is_valid_origin && !is_valid_referer {
        return Err(error::ErrorForbidden("Security Alert: CSRF detected"));
    }
    Ok(())
}

// Safe HTMX 4 Header Inspection:
// Returns true ONLY when HTMX expects a partial HTML fragment.
// On browser Back/Forward navigation, HTMX 4 sends `hx-history-restore-request: true`
// (or `hx-request-type: full`) and expects the full page containing `<main hx-history-elt>`.
pub fn is_htmx_partial_request(req: &HttpRequest) -> bool {
    let is_htmx =
        req.headers().contains_key("hx-request") || req.headers().contains_key("hx-request-type");
    let is_history_restore = req.headers().contains_key("hx-history-restore-request");
    let is_full_request = req
        .headers()
        .get("hx-request-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "full");

    is_htmx && !is_history_restore && !is_full_request
}

// Memory-Safe UTF-8 Character Truncation:
// Iterates over Unicode scalar values (`.chars()`) rather than raw byte slices,
// preventing server panics on multi-byte UTF-8 character boundaries and emojis.
pub fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() > max {
        let mut truncated = s.chars().take(max).collect::<String>().trim_end().to_string();
        truncated.push_str("...");
        truncated
    } else {
        s.to_string()
    }
}

// Anti-XSS Escaping for Dynamic <title> Tags in HTMX Partial Responses:
// Since partial HTMX responses prepend `<title>` outside `BaseTemplate`, this function
// sanitizes HTML control characters (&, <, >) to prevent tag injection and XSS.
pub fn html_escape_title(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// Secure SSR & Partial HTML Renderer:
// Explicitly enforces `charset=utf-8` (mitigating charset-sniffing XSS) and sets
// `Cache-Control: no-store, no-cache, must-revalidate` to prevent stale sensitive state caching.
pub fn render_page(
    req: &HttpRequest,
    partial: String,
    path: &str,
    page_title: &str,
    page_description: &str,
) -> Result<HttpResponse, Error> {
    if is_htmx_partial_request(req) {
        let safe_title = html_escape_title(page_title);
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .body(format!("<title>{safe_title}</title>\n{partial}")))
    } else {
        let base = BaseTemplate {
            page_title: page_title.to_string(),
            page_description: page_description.to_string(),
            content_html: partial,
            current_path: path.to_string(),
        };
        let body = base.render().map_err(error::ErrorInternalServerError)?;
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .body(body))
    }
}

// Safe 404 Not Found Renderer:
// Returns a genuine HTTP 404 status code without leaking internal database details or stack traces.
pub fn render_not_found(req: &HttpRequest, current_path: &str) -> Result<HttpResponse, Error> {
    let partial = PostDetailTemplate { post: None }
        .render()
        .map_err(error::ErrorInternalServerError)?;
    let page_title = "Post not found — Add Post (SAHAR)";

    if is_htmx_partial_request(req) {
        Ok(HttpResponse::NotFound()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .body(format!("<title>{page_title}</title>\n{partial}")))
    } else {
        let base = BaseTemplate {
            page_title: page_title.to_string(),
            page_description: "The requested post was not found.".to_string(),
            content_html: partial,
            current_path: current_path.to_string(),
        };
        let body = base.render().map_err(error::ErrorInternalServerError)?;
        Ok(HttpResponse::NotFound()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .body(body))
    }
}