# Add Post (SAHAR) — v1.1.0 (Релиз безопасности, целостности данных и совместимости с W3C HTML5)

[![En](https://img.shields.io/badge/En-black?style=flat&logoColor=white)](https://github.com/AndrewShedov/add-post--SAHAR/releases/tag/v1.1.0)<br>

Данный релиз существенно усиливает защиту прототипа от утечек внутренней информации (CWE-209), гонок TOCTOU в ScyllaDB и векторов CSRF/XSS/HTMX-инъекций, добавляет Fail-Fast конфигурацию через переменные окружения, исправляет баг удаления карточек в HTMX 4 и граничные случаи восстановления скролла, а также достигает **0 ошибок и 0 предупреждений в [validator.w3.org](https://validator.w3.org/nu/)**.

---

### Полный список изменений

#### 🔒 Усиление кибербезопасности
* **Исключение утечки внутренней информации (CWE-209):**
  * Навешены кастомные обработчики `.error_handler(...)` на `FormConfig`, `QueryConfig` и `PathConfig` в `src/main.rs`, возвращающие безликое `"Bad Request"` без раскрытия внутренних типов Rust/Serde и размеров буфера.
  * Все вызовы `.map_err(error::ErrorInternalServerError)` в `src/handlers.rs` и `src/security.rs` заменены на `internal_error()`: подробности пишутся строго в `stderr` (`eprintln!`), клиенту отдаётся `"Internal Server Error"`.
  * Функция `validate_origin()` пишет причину блокировки только в серверный лог, возвращая клиенту обезличенное `"Forbidden"`.
* **Fail-Fast конфигурация и Fail-Closed Anti-CSRF (`src/main.rs`, `src/security.rs`):**
  * Перед `HttpServer::new(...)` добавлен вызов `security::init_security_config()`, который прогревает `OnceLock` и останавливает запуск при некорректных значениях `CANONICAL_BASE_URL` или `ALLOWED_ORIGINS`.
  * Заголовки `Sec-Fetch-Site`, `Origin` и `Referer` с невалидными не-ASCII байтами сразу отклоняются с `403 Forbidden`.
  * Требуется строгое `Sec-Fetch-Site: same-origin`, заголовок `Origin` проверяется в безусловном приоритете над `Referer` (блокируя `Origin: null`), сравнения нормализуются через `.to_ascii_lowercase()`, а `Referer` проверяется строго по границе authority (RFC 3986: `/`, `?`, `#`).
* **Усиленные CSP и HTTP-заголовки безопасности (`src/main.rs`, `src/security.rs`):**
  * Политика CSP расширена до: `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none';`.
  * Добавлены `Permissions-Policy`, `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Resource-Policy: same-origin` и `X-Permitted-Cross-Domain-Policies: none`.
  * Добавлен заголовок `Vary: HX-Request, HX-Request-Type, HX-History-Restore-Request` для изоляции частичных HTMX-фрагментов от полных SSR-страниц в кэшах.
  * Для защиты от Slowloris заданы таймауты `HttpServer`: `client_request_timeout(5s)` и `keep_alive(30s)`.
* **Однопроходная санитизация ввода и защита шаблонов (`src/security.rs`, `templates/*.html`):**
  * Реализована однопроходная функция `sanitize_text_input()`, нормализующая `\r\n`, `\r`, `\u{2028}`, `\u{2029}` → `\n`, удаляющая `\0`, управляющие символы, невидимые пробелы/BOM (`\u{200B}`, `\u{2060}`, `\u{FEFF}`) и BiDi-переопределения, сохраняющая ZWJ/ZWNJ (`\u{200D}`, `\u{200C}`) для составных эмодзи (`👨‍💻`, `❤️‍🔥`) и срезающая невидимые горизонтальные символы (`is_inline_blank`) перед переносами строки для защиты от флуда вида `"\n \n\u{2800}\n"`.
  * Функция `html_escape_title()` расширена экранированием `"` и `'` в дополнение к `&`, `<`, `>`.
  * На заголовки и тексты постов в `post_items.html` и `post_detail.html` добавлены `data-hx-ignore` и `dir="auto"`.
  * На формы (`home.html`, `post_edit.html`) и кнопки удаления (`post_items.html`, `post_detail.html`) добавлены `data-hx-sync="this:drop"` и `data-hx-disable`.

#### 🛡️ Целостность данных, ScyllaDB и отказоустойчивость
* **Единый маршрут удаления (`DELETE /api/posts/{id}`):** Заменён `/api/posts/{created_at}/{id}`, чтобы `bucket` и `created_at` всегда брались из эталонной записи ScyllaDB (`posts_by_id`).
* **Атомарная вставка через `BatchType::Logged` (`src/db.rs`, `src/handlers.rs`):** Предсобранный `batch_insert_post` в `AppState` атомарно записывает данные в `posts` и `posts_by_id` через `batchlog` координатора.
* **Согласованность через двойной LWT (`IF EXISTS` / `IF NOT EXISTS`):** Условие `IF EXISTS` применено на обеих таблицах (`posts_by_id` и `posts`) в `update_post` и `delete_post`, добавлены `is_lwt_applied()` и Paxos-откат `stmt_restore_post_by_id` (`INSERT ... IF NOT EXISTS`), что полностью исключает гонки TOCTOU, «зомби-строки», «посты-призраки» и двойной декремент счётчика.
* **Честная обработка сбоев БД и сохранение текста форм:** Внедрены `PostLookup` (`Found`, `NotFound`, `Unavailable`), `get_total_posts() -> Option<i64>`, заголовок `Retry-After: 5` на `503 Service Unavailable` и `toast_error_response()` (`HX-Reswap: none`) для ответов `400`/`404`/`503`, чтобы набранный текст пользователя не пропадал при ошибке.
* **Постраничная синхронизация на старте (`src/db.rs`):** Непостраничный скан в `sync_on_startup` заменён на `query_single_page` + `PagingState` (`page_size = 500`), добавлен 5-секундный таймаут подключения к ScyllaDB.

#### 🐛 Исправления ошибок, валидность W3C HTML5, SEO и качество кода
* **Полная валидность W3C Nu Html Checker (`templates/*.html`):** Все шаблоны переведены на нативный HTML5-префикс `data-hx-*`, удалён избыточный `<meta name="htmx-config">` (HTMX 4 использует совместимый с CSP `document.adoptedStyleSheets`), убраны закрывающие слэши (`/>`) у void-элементов — достигнут результат **0 ошибок, 0 предупреждений и 0 `Info`-замечаний**.
* **Фикс удаления карточек в `/blog` и сброса скролла (`static/app.js`):** Удаление `.post-item` перенесено в `htmx:finally:request` с проверкой числового `ctx.response.status` (`2xx` / `404`), на кнопки удаления в ленте добавлен `data-hx-swap="none"`, а сброс состояния скролла/кэша перенесён в `htmx:before:request` (после подтверждения `data-hx-confirm`) и событие `submit` форм.
* **Управление SEO `noindex` (`src/models.rs`, `templates/base.html`, `src/handlers.rs`):** Поле `is_not_found` заменено на `noindex: bool` — и `404`, и `/posts/{id}/edit` выводят `<meta name="robots" content="noindex, nofollow">` без `<link rel="canonical">`.
* **Опциональная runtime-конфигурация (`src/main.rs`, `src/db.rs`, `src/security.rs`):** Добавлена настройка через `BIND_ADDR`, `SCYLLA_URI` и кэшируемые в `OnceLock` переменные `CANONICAL_BASE_URL` и `ALLOWED_ORIGINS` с полным сохранением дефолтного Zero-Config запуска.
* **Чистка кода и Rust 2024:** Версия в `Cargo.toml` обновлена до `1.1.0`, удалено неиспользуемое поле `Post.id` в `src/models.rs`, `.unwrap()` в `templates/post_items.html` заменён на `{% if let Some(ts) = next_timestamp %}`, добавлено версионирование статики `?v=1.1.0`, удалена неиспользуемая CSS-переменная `--desktop_menu_part_2_wrap_border-bottom` в `static/style.css`, добавлен `type="button"` на `.toast-close`, удалён дубликат `./favicon.ico` из корня проекта, вложенные `if let` свёрнуты в `let`-цепочки Rust 2024, и получен **чистый 0 предупреждений** в `cargo clippy -- -D warnings`.