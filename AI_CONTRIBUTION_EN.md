# Add Post (SAHAR)

[![Ru](https://img.shields.io/badge/Ru-black?style=flat&logoColor=white)](https://github.com/AndrewShedov/add-post--SAHAR/blob/main/AI_CONTRIBUTION_RU.md)<br>

### ***AI Model Contribution Breakdown ✨***
* **Gemini 4 Argon** — core architecture & cybersecurity (60%) **|v1.0.0|**
* **Gemini 4 Hydrogen** — cybersecurity & data integrity (22%) **|v1.1.0|**
* **Gemini 4 Carbon** — cybersecurity (10%) **|v1.1.0|**
* **Claude Fable 5.2** — cybersecurity (8%) **|v1.1.0|**

---

#### 1. Gemini 4 Argon — Core Architecture & Cybersecurity (60%)
* Designed and implemented the full multi-page **SAHAR** prototype, including 3-table denormalization, cursor-based pagination, SSR + SPA navigation, and ephemeral smart scroll restoration.
* **Cybersecurity foundations implemented:**
  * **100% CQL Injection Immunity:** All database operations execute strictly via pre-compiled `PreparedStatement` queries (`src/db.rs`, `src/handlers.rs`).
  * **Compile-Time XSS Protection:** Askama templates auto-escape all user content at build time; multi-line text renders safely via CSS `white-space: pre-wrap` without unescaped HTML tags.
  * **Partial `<title>` XSS Sanitization:** Built `html_escape_title()` for HTMX partial swaps.
  * **Baseline Strict CSP:** Enforced `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self';` without `'unsafe-inline'` or `'unsafe-eval'`.
  * **Baseline Anti-CSRF:** Added `validate_origin()` allowlist verification on mutating endpoints (`POST` / `DELETE`).
  * **Defense-in-Depth Input & Buffer Limits:** Added `web::FormConfig::limit(65536)` (64 KB), server-side `.chars()` length caps, and HTML `maxlength` constraints.
  * **UTF-8 Panic Immunity (DoS via Panic):** Used Unicode scalar `.chars()` iteration in `truncate()` and form handlers to prevent server crashes on emojis and multi-byte boundaries.
  * **Perimeter Headers & Genuine 404:** Configured `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Strict-Transport-Security`, `Referrer-Policy`, `Cache-Control: no-store, no-cache, must-revalidate`, explicit `charset=utf-8`, genuine `HTTP 404 Not Found` status, and non-negative counter bounds.

#### 2. Gemini 4 Hydrogen — Cybersecurity & Data Integrity (22%)
* **What "Data Integrity" means here:**
  Ensuring that all three denormalized ScyllaDB tables (`posts`, `posts_by_id`, `post_counts`) and the browser DOM remain strictly synchronized under concurrent requests, malformed inputs, or partial database failures — eliminating ghost posts, corrupted zombie rows with `NULL` columns, counter drift, and misleading UI deletions.
* **Cybersecurity & Data Integrity upgrades implemented:**
  * **Authoritative Deletion & Ghost-Post Fix (`DELETE /api/posts/{id}`):** Removed `created_at` from the delete URL; `delete_post` now resolves authoritative `bucket` and `created_at` directly from ScyllaDB (`posts_by_id`), preventing table desynchronization caused by spoofed or stale URL timestamps.
  * **Atomic Creation (`BatchType::Logged`) & Dual-Table LWT Execution:** Pre-built `batch_insert_post` (`BatchType::Logged`) in `AppState` for atomic writes across `posts` and `posts_by_id` via coordinator `batchlog`; added `is_lwt_applied()` helper and LWT execution across updates and deletes.
  * **HTMX 4 Safe DOM Removal & Scroll Cache Fix:** Fixed a bug where `.post-item` cards remained in `/blog` after deletion because HTMX 4's `ctx.response` does not expose `.ok` and `HX-Reswap: none` skips swaps. Moved card removal to `htmx:finally:request` checking numeric HTTP status (`2xx` or `404`), and moved scroll/cache invalidation to `htmx:before:request` (after `data-hx-confirm` acceptance) and form `submit` events.
  * **Control Character & Log/Terminal Injection Sanitization:** Introduced `sanitize_text_input()` stripping `\0` and control characters while normalizing line endings.
  * **Slowloris & Paginated Startup Protection:** Added `HttpServer` timeouts (`client_request_timeout(5s)`, `keep_alive(30s)`), a `5s` ScyllaDB connection timeout, and paginated `sync_on_startup` (`query_single_page`, `page_size = 500`).
  * **Zero-Config Environment Configuration:** Added runtime configuration via `BIND_ADDR`, `SCYLLA_URI`, and `OnceLock`-cached `CANONICAL_BASE_URL` and `ALLOWED_ORIGINS` (normalized via `.to_ascii_lowercase()`) without external dependencies.
  * **Extended Hardening & Full W3C HTML5 Compliance:** Added `connect-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'` to CSP; added `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Resource-Policy: same-origin`, `X-Permitted-Cross-Domain-Policies: none`, and `Vary` cache-isolation headers; extended `html_escape_title()` to escape `"` and `'`; migrated templates to W3C-valid `data-hx-*` attributes, removed redundant `<meta name="htmx-config">` (relying on HTMX 4's CSP-safe `document.adoptedStyleSheets`), removed void-element trailing slashes (`/>`), and added static asset cache-busting (`?v=1.1.0`).

#### 3. Gemini 4 Carbon — Cybersecurity (10%)
* **`Permissions-Policy` Header:** Restricted access to sensitive browser APIs (`geolocation=(), microphone=(), camera=(), payment=(), usb=(), interest-cohort=()`).
* **HTMX-Injection & BiDi Defense Concept:** Proposed isolating user-generated content blocks against future attribute injection (`data-hx-ignore`) and adding `dir="auto"` against RTL/BiDi layout spoofing.
* **Cross-Audit Catches Implemented in `v1.1.0`:**
  * Identified CWE-209 raw error leakage on default Actix Web extractors (`FormConfig`, `QueryConfig`, `PathConfig`) and direct template `.map_err(ErrorInternalServerError)` forwarding.
  * Identified the `update_post` vs `delete_post` CQL `UPSERT` zombie-row race, prompting `IF EXISTS` across **both** tables (`posts_by_id` and `posts`) and LWT compensating rollback (`INSERT ... IF NOT EXISTS`).
  * Flagged Fail-Fast startup config validation (`init_security_config()`), Fail-Closed non-ASCII header rejection in `validate_origin()`, ZWJ (`\u{200D}`) emoji preservation and `is_inline_blank` newline-flood hardening in `sanitize_text_input()`, `503` separation with `Retry-After: 5`, `400`/`503` form preservation via `toast_error_response`, `noindex` on `/posts/{id}/edit`, `data-hx-swap="none"` on delete buttons, `if let Some(ts)` zero-unwrap template syntax, `Cargo.toml` `1.1.0` version bump, and redundant root `favicon.ico` removal.

#### 4. Claude Fable 5.2 — Cybersecurity (8%)
* **Hardened Anti-CSRF Rules:** Strict `Sec-Fetch-Site == "same-origin"` enforcement (blocking cross-site and sibling-subdomain origins), strict `Origin`-first priority over `Referer` (blocking `Origin: null` from sandboxed iframes), and RFC 3986 authority-boundary `Referer` validation.
* **`base-uri 'none'` CSP Directive:** Blocked `<base>` tag hijacking vectors.
* **Blank-Line Flood Mitigation:** Pattern for collapsing more than two consecutive newlines (`\n\n`) to prevent vertical feed stretching.
* **`data-hx-sync="this:drop"`:** Native HTMX request-queue duplicate-submission protection on forms and delete buttons.
* **Internal Error Suppression Pattern:** Logging diagnostic error details strictly to server `stderr` (`eprintln!`) while returning generic HTTP error text to clients.

---