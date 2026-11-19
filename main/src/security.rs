use crate::models::{BaseTemplate, PostDetailTemplate};
use actix_web::{error, Error, HttpRequest, HttpResponse};
use askama::Template;
use std::fmt::Display;
use std::sync::OnceLock;

static ALLOWED_ORIGINS: OnceLock<Vec<String>> = OnceLock::new();
static CANONICAL_BASE_URL: OnceLock<String> = OnceLock::new();

pub const VARY_HTMX_HEADERS: &str = "HX-Request, HX-Request-Type, HX-History-Restore-Request";

// CWE-209 & OWASP A09 Mitigation:
// Logs internal error details strictly to server stderr and returns a generic error to the client.
pub fn internal_error<E: Display>(context: &str, err: E) -> Error {
    eprintln!("❌ [{context}]: {err}");
    error::ErrorInternalServerError("Internal Server Error")
}

pub fn service_unavailable_response() -> HttpResponse {
    HttpResponse::ServiceUnavailable()
        .content_type("text/plain; charset=utf-8")
        .insert_header(("Retry-After", "5"))
        .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
        .body("Service Unavailable")
}

// Allowlist of trusted Origins for Cross-Site Request Forgery (CSRF) protection.
// Reads `ALLOWED_ORIGINS` (comma-separated) from env or falls back to local prototype origins.
fn allowed_origins() -> &'static [String] {
    ALLOWED_ORIGINS.get_or_init(|| {
        if let Ok(raw) = std::env::var("ALLOWED_ORIGINS") {
            let parsed: Vec<String> = raw
                .split(',')
                .map(|s| s.trim().trim_end_matches('/').to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            if !parsed.is_empty() {
                return parsed;
            }
        }
        vec![
            "http://127.0.0.1:8080".to_string(),
            "http://localhost:8080".to_string(),
        ]
    })
}

pub fn canonical_base_url() -> &'static str {
    CANONICAL_BASE_URL.get_or_init(|| {
        std::env::var("CANONICAL_BASE_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:8080".to_string())
    })
}

fn validate_base_origin(value: &str) -> bool {
    let authority = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or("");
    !authority.is_empty() && !authority.contains(['/', '?', '#', '@', ' '])
}

// Fail-Fast startup warmup & strict validation of runtime security config
pub fn init_security_config() {
    let base = canonical_base_url();
    assert!(
        validate_base_origin(base),
        "Invalid CANONICAL_BASE_URL '{base}': must be exact scheme://host[:port] with no path, query, or fragment"
    );

    for origin in allowed_origins() {
        assert!(
            validate_base_origin(origin),
            "Invalid ALLOWED_ORIGINS entry '{origin}': must be exact scheme://host[:port] with no path or query"
        );
    }
}

// Fail-Closed Anti-CSRF (Cross-Site Request Forgery) Validation:
// Invoked on all state-mutating endpoints (POST / DELETE).
// Rejects malformed non-ASCII headers immediately (fail-closed), enforces `Sec-Fetch-Site: same-origin`
// (when provided), checks `Origin` with strict priority (blocking `Origin: null` and foreign origins),
// and validates `Referer` strictly along RFC 3986 authority boundaries when `Origin` is absent.
pub fn validate_origin(req: &HttpRequest) -> Result<(), Error> {
    if let Some(raw_fetch_site) = req.headers().get("Sec-Fetch-Site") {
        let Ok(fetch_site) = raw_fetch_site.to_str() else {
            eprintln!("🛡️ [CSRF BLOCKED] Malformed non-ASCII Sec-Fetch-Site header");
            return Err(error::ErrorForbidden("Forbidden"));
        };
        if fetch_site != "same-origin" {
            eprintln!("🛡️ [CSRF BLOCKED] Invalid Sec-Fetch-Site: {fetch_site:?}");
            return Err(error::ErrorForbidden("Forbidden"));
        }
    }

    let origins = allowed_origins();

    if let Some(raw_origin) = req.headers().get("Origin") {
        let Ok(origin) = raw_origin.to_str() else {
            eprintln!("🛡️ [CSRF BLOCKED] Malformed non-ASCII Origin header");
            return Err(error::ErrorForbidden("Forbidden"));
        };
        let normalized_origin = origin.to_ascii_lowercase();
        if !origins.iter().any(|allowed| allowed == &normalized_origin) {
            eprintln!("🛡️ [CSRF BLOCKED] Untrusted Origin: {origin:?}");
            return Err(error::ErrorForbidden("Forbidden"));
        }
        return Ok(());
    }

    let Some(raw_referer) = req.headers().get("Referer") else {
        eprintln!("🛡️ [CSRF BLOCKED] Missing both Origin and Referer headers");
        return Err(error::ErrorForbidden("Forbidden"));
    };
    let Ok(referer) = raw_referer.to_str() else {
        eprintln!("🛡️ [CSRF BLOCKED] Malformed non-ASCII Referer header");
        return Err(error::ErrorForbidden("Forbidden"));
    };

    let referer_lower = referer.to_ascii_lowercase();
    let is_valid_referer = origins.iter().any(|allowed| {
        referer_lower == *allowed
            || referer_lower.strip_prefix(allowed.as_str()).is_some_and(|rest| {
                rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('#')
            })
    });

    if !is_valid_referer {
        eprintln!("🛡️ [CSRF BLOCKED] Missing Origin and untrusted Referer: {referer:?}");
        return Err(error::ErrorForbidden("Forbidden"));
    }

    Ok(())
}

fn is_stripped_unicode(ch: char) -> bool {
    matches!(
        ch,
        '\0'
            | '\u{061C}' // Arabic Letter Mark
            | '\u{200B}' // Zero-Width Space (preserves \u{200C} ZWNJ and \u{200D} ZWJ for compound emojis!)
            | '\u{200E}'
            | '\u{200F}' // LRM / RLM
            | '\u{202A}'..='\u{202E}' // BiDi embedding & override
            | '\u{2060}' // Word Joiner
            | '\u{2066}'..='\u{2069}' // BiDi isolates
            | '\u{FEFF}' // BOM / Zero-Width No-Break Space
    )
}

fn is_edge_blank(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '\u{2800}' | '\u{3164}' | '\u{FFA0}' | '\u{115F}' | '\u{1160}'
        )
}

fn is_inline_blank(ch: char) -> bool {
    ch != '\n' && is_edge_blank(ch)
}

// Single-Pass Input Sanitization, CRLF Normalization & Blank-Line Flood Protection:
// Strips null bytes, control chars, and dangerous BiDi/zero-width overrides while preserving ZWJ
// compound emojis, normalizes CRLF/CR/\u{2028}/\u{2029} -> LF, collapses blank-line floods
// (even with spaces/tabs/blank chars via `is_inline_blank`), and truncates safely by Unicode scalars.
pub fn sanitize_text_input(input: &str, max_chars: usize, allow_newlines: bool) -> String {
    let mut cleaned = String::with_capacity(input.len().min(max_chars * 4));
    let mut consecutive_newlines: usize = 0;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\r' || ch == '\n' || ch == '\u{2028}' || ch == '\u{2029}' {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            if allow_newlines {
                while let Some(last) = cleaned.chars().next_back()
                    && is_inline_blank(last)
                {
                    cleaned.pop();
                }
                if !cleaned.is_empty() {
                    consecutive_newlines += 1;
                    if consecutive_newlines <= 2 {
                        cleaned.push('\n');
                    }
                }
            } else if !cleaned.is_empty() && !cleaned.ends_with(' ') {
                cleaned.push(' ');
            }
            continue;
        }

        if ch == '\t' {
            if allow_newlines {
                cleaned.push('\t');
            } else if !cleaned.is_empty() && !cleaned.ends_with(' ') {
                cleaned.push(' ');
            }
            continue;
        }

        if is_stripped_unicode(ch) || ch.is_control() {
            continue;
        }

        if !is_edge_blank(ch) {
            consecutive_newlines = 0;
        }
        cleaned.push(ch);
    }

    let trimmed = cleaned.trim_matches(is_edge_blank);
    trimmed
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim_end_matches(is_edge_blank)
        .to_string()
}

// Safe HTMX 4 Header Inspection:
// Returns true ONLY when HTMX expects a partial HTML fragment.
// On browser Back/Forward navigation, HTMX 4 sends `hx-history-restore-request: true`
// (or `hx-request-type: full`) and expects the full page containing `<main data-hx-history-elt>`.
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
// sanitizes HTML control characters (&, <, >, ", ') to prevent tag injection and XSS.
pub fn html_escape_title(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// Secure SSR & Partial HTML Renderer:
// Explicitly enforces `charset=utf-8` (mitigating charset-sniffing XSS), sets
// `Cache-Control: no-store, no-cache, must-revalidate`, and isolates HTMX vs full-page caches via `Vary`.
pub fn render_page(
    req: &HttpRequest,
    partial: String,
    path: &str,
    page_title: &str,
    page_description: &str,
    noindex: bool,
) -> Result<HttpResponse, Error> {
    if is_htmx_partial_request(req) {
        let safe_title = html_escape_title(page_title);
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .insert_header(("Vary", VARY_HTMX_HEADERS))
            .body(format!("<title>{safe_title}</title>\n{partial}")))
    } else {
        let base = BaseTemplate {
            page_title: page_title.to_string(),
            page_description: page_description.to_string(),
            content_html: partial,
            current_path: path.to_string(),
            canonical_base_url: canonical_base_url().to_string(),
            noindex,
        };
        let body = base
            .render()
            .map_err(|e| internal_error("TEMPLATE base", e))?;
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .insert_header(("Vary", VARY_HTMX_HEADERS))
            .body(body))
    }
}

// Safe 404 Not Found Renderer:
// Returns a genuine HTTP 404 status code without leaking internal database details or stack traces,
// and suppresses canonical/index signals so arbitrary invalid URLs are never indexed.
pub fn render_not_found(req: &HttpRequest, current_path: &str) -> Result<HttpResponse, Error> {
    let partial = PostDetailTemplate { post: None }
        .render()
        .map_err(|e| internal_error("TEMPLATE post_detail 404", e))?;
    let page_title = "Post not found — Add Post (SAHAR)";

    if is_htmx_partial_request(req) {
        let safe_title = html_escape_title(page_title);
        Ok(HttpResponse::NotFound()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .insert_header(("Vary", VARY_HTMX_HEADERS))
            .body(format!("<title>{safe_title}</title>\n{partial}")))
    } else {
        let base = BaseTemplate {
            page_title: page_title.to_string(),
            page_description: "The requested post was not found.".to_string(),
            content_html: partial,
            current_path: current_path.to_string(),
            canonical_base_url: canonical_base_url().to_string(),
            noindex: true,
        };
        let body = base
            .render()
            .map_err(|e| internal_error("TEMPLATE base 404", e))?;
        Ok(HttpResponse::NotFound()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .insert_header(("Vary", VARY_HTMX_HEADERS))
            .body(body))
    }
}