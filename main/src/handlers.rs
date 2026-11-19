use crate::db::{fetch_post_by_uuid, get_total_posts, is_lwt_applied, AppState, PostLookup};
use crate::models::{
    BlogTemplate, DeleteQuery, EditQuery, HomeTemplate, PagingQuery, Post, PostDetailTemplate,
    PostEditTemplate, PostForm, PostItemsTemplate, ToastTemplate,
};
use crate::security::{
    html_escape_title, internal_error, is_htmx_partial_request, render_not_found, render_page,
    sanitize_text_input, service_unavailable_response, truncate, validate_origin,
    VARY_HTMX_HEADERS,
};
use actix_web::http::StatusCode;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use askama::Template;
use chrono::{DateTime, Utc};
use scylla::value::CqlTimestamp;
use uuid::Uuid;

fn toast_error_response(
    status: StatusCode,
    message: &str,
    retry_after: bool,
) -> Result<HttpResponse, Error> {
    let toast_html = ToastTemplate {
        message: message.to_string(),
        is_success: false,
    }
    .render()
    .map_err(|e| internal_error("TEMPLATE toast", e))?;

    let mut builder = HttpResponse::build(status);
    builder
        .content_type("text/html; charset=utf-8")
        .insert_header(("HX-Reswap", "none"))
        .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"));
    if retry_after {
        builder.insert_header(("Retry-After", "5"));
    }
    Ok(builder.body(toast_html))
}

async fn render_blog_feed_html(
    state: &web::Data<AppState>,
    before: Option<i64>,
) -> Result<String, Box<HttpResponse>> {
    let mut posts = Vec::new();
    let mut next_timestamp = None;

    let res = if let Some(before_ts) = before {
        state
            .session
            .execute_unpaged(
                &state.stmt_get_posts_paged,
                (1_i32, CqlTimestamp(before_ts)),
            )
            .await
    } else {
        state
            .session
            .execute_unpaged(&state.stmt_get_posts, (1_i32,))
            .await
    };

    let res_data = match res {
        Ok(data) => data,
        Err(err) => {
            eprintln!("❌ [ScyllaDB render_blog_feed_html]: {err}");
            return Err(Box::new(service_unavailable_response()));
        }
    };

    if let Ok(rows) = res_data.into_rows_result()
        && let Ok(row_iter) = rows.rows::<(CqlTimestamp, Uuid, String, String)>()
    {
        for (ts, id, title, content) in row_iter.flatten() {
            let dt = DateTime::<Utc>::from_timestamp_millis(ts.0).unwrap_or_default();
            posts.push(Post {
                id_hex: id.simple().to_string(),
                // Limit preview card title to 100 chars and post text to 730 chars in the /blog feed
                title: truncate(&title, 100),
                content: truncate(&content, 730),
                created_at: ts.0,
                date_str: dt.format("%d %b %Y, %H:%M").to_string(),
            });
        }
    }

    if posts.len() == 5 {
        next_timestamp = posts.last().map(|p| p.created_at);
    }

    PostItemsTemplate {
        posts,
        next_timestamp,
    }
    .render()
    .map_err(|e| {
        eprintln!("❌ [TEMPLATE post_items]: {e}");
        Box::new(
            HttpResponse::InternalServerError()
                .content_type("text/plain; charset=utf-8")
                .body("Internal Server Error"),
        )
    })
}

pub async fn index(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let Some(total_posts) = get_total_posts(&state).await else {
        return Ok(service_unavailable_response());
    };
    let partial = HomeTemplate { total_posts }
        .render()
        .map_err(|e| internal_error("TEMPLATE home", e))?;
    render_page(
        &req,
        partial,
        "/",
        "Add Post (SAHAR)",
        "Create a new post in the SAHAR stack prototype.",
        false,
    )
}

pub async fn blog(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PagingQuery>,
) -> Result<HttpResponse, Error> {
    // Loader Delay
    // if crate::LOADER_DELAY_MS > 0 {
    //     tokio::time::sleep(std::time::Duration::from_millis(crate::LOADER_DELAY_MS)).await;
    // }
    // /Loader Delay

    let items_html = match render_blog_feed_html(&state, query.before).await {
        Ok(html) => html,
        Err(resp) => return Ok(*resp),
    };

    if is_htmx_partial_request(&req) && query.before.is_some() {
        return Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .insert_header(("Vary", VARY_HTMX_HEADERS))
            .body(items_html));
    }

    let partial = BlogTemplate {
        posts_html: items_html,
    }
    .render()
    .map_err(|e| internal_error("TEMPLATE blog", e))?;

    let current_path = match query.before {
        Some(ts) => format!("/blog?before={ts}"),
        None => "/blog".to_string(),
    };

    render_page(
        &req,
        partial,
        &current_path,
        "Blog — Add Post (SAHAR)",
        "Read the latest posts in the SAHAR stack prototype.",
        false,
    )
}

pub async fn view_post(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let raw_id = path.into_inner();
    let current_path = format!("/posts/{raw_id}");

    let Ok(post_uuid) = Uuid::parse_str(&raw_id) else {
        return render_not_found(&req, &current_path);
    };

    let post = match fetch_post_by_uuid(&state, post_uuid).await {
        PostLookup::Found(_, post) => post,
        PostLookup::NotFound => return render_not_found(&req, &current_path),
        PostLookup::Unavailable => return Ok(service_unavailable_response()),
    };

    let canonical_path = format!("/posts/{}", post.id_hex);
    let seo_title = if post.title.is_empty() {
        format!(
            "{} — Add Post (SAHAR)",
            truncate(&post.content, 40).replace(['\n', '\r', '\t'], " ")
        )
    } else {
        format!("{} — Add Post (SAHAR)", post.title)
    };
    let seo_desc = truncate(&post.content, 150).replace(['\n', '\r', '\t'], " ");

    let partial = PostDetailTemplate { post: Some(post) }
        .render()
        .map_err(|e| internal_error("TEMPLATE post_detail", e))?;

    render_page(
        &req,
        partial,
        &canonical_path,
        &seo_title,
        &seo_desc,
        false,
    )
}

pub async fn edit_post_page(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<EditQuery>,
) -> Result<HttpResponse, Error> {
    let raw_id = path.into_inner();
    let current_path = format!("/posts/{raw_id}/edit");

    let Ok(post_uuid) = Uuid::parse_str(&raw_id) else {
        return render_not_found(&req, &current_path);
    };

    let post = match fetch_post_by_uuid(&state, post_uuid).await {
        PostLookup::Found(_, post) => post,
        PostLookup::NotFound => return render_not_found(&req, &current_path),
        PostLookup::Unavailable => return Ok(service_unavailable_response()),
    };

    let canonical_path = format!("/posts/{}/edit", post.id_hex);
    let back_url = match query.from.as_deref() {
        Some("blog") => "/blog".to_string(),
        _ => format!("/posts/{}", post.id_hex),
    };

    let partial = PostEditTemplate {
        post: Some(post),
        back_url,
    }
    .render()
    .map_err(|e| internal_error("TEMPLATE post_edit", e))?;

    render_page(
        &req,
        partial,
        &canonical_path,
        "Edit post — Add Post (SAHAR)",
        "Edit post in the SAHAR stack prototype.",
        true,
    )
}

pub async fn create_post(
    req: HttpRequest,
    state: web::Data<AppState>,
    form: web::Form<PostForm>,
) -> Result<HttpResponse, Error> {
    validate_origin(&req)?;

    let title = sanitize_text_input(&form.title, 150, false);
    let content = sanitize_text_input(&form.content, 3000, true);

    if content.is_empty() {
        return toast_error_response(StatusCode::BAD_REQUEST, "Content is required", false);
    }

    let post_id = Uuid::new_v4();
    let now_ms = Utc::now().timestamp_millis();
    let cql_ts = CqlTimestamp(now_ms);

    let batch_res = state
        .session
        .batch(
            &state.batch_insert_post,
            (
                (1_i32, cql_ts, post_id, &title, &content),
                (post_id, 1_i32, cql_ts, &title, &content),
            ),
        )
        .await;

    if let Err(err) = batch_res {
        eprintln!("❌ [ScyllaDB create_post batch {post_id}]: {err}");
        // Preserve user's typed text in the form on DB failure
        return toast_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Failed to add post",
            true,
        );
    }

    if let Err(err) = state
        .session
        .execute_unpaged(&state.stmt_inc_counter, (1_i32,))
        .await
    {
        eprintln!("❌ [ScyllaDB increment counter]: {err}");
    }

    let total_posts = get_total_posts(&state).await.unwrap_or(0);

    let form_html = HomeTemplate { total_posts }
        .render()
        .map_err(|e| internal_error("TEMPLATE home", e))?;
    let toast_html = ToastTemplate {
        message: "Post added successfully".to_string(),
        is_success: true,
    }
    .render()
    .map_err(|e| internal_error("TEMPLATE toast", e))?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
        .body(format!("{form_html}\n{toast_html}")))
}

pub async fn update_post(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    form: web::Form<PostForm>,
) -> Result<HttpResponse, Error> {
    validate_origin(&req)?;

    let raw_id = path.into_inner();
    let Ok(post_uuid) = Uuid::parse_str(&raw_id) else {
        return toast_error_response(StatusCode::BAD_REQUEST, "Invalid post ID", false);
    };

    let title = sanitize_text_input(&form.title, 150, false);
    let content = sanitize_text_input(&form.content, 3000, true);

    if content.is_empty() {
        return toast_error_response(StatusCode::BAD_REQUEST, "Content is required", false);
    }

    let (bucket, existing_post) = match fetch_post_by_uuid(&state, post_uuid).await {
        PostLookup::Found(bucket, post) => (bucket, post),
        PostLookup::NotFound => {
            return toast_error_response(
                StatusCode::NOT_FOUND,
                "Post not found or already deleted",
                false,
            );
        }
        PostLookup::Unavailable => {
            return toast_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Failed to update post",
                true,
            );
        }
    };

    let cql_ts = CqlTimestamp(existing_post.created_at);

    // Execute LWT (IF EXISTS) on posts_by_id first to guard against concurrent deletion
    match state
        .session
        .execute_unpaged(&state.stmt_update_post_by_id, (&title, &content, post_uuid))
        .await
    {
        Ok(res) => {
            if !is_lwt_applied(res) {
                return toast_error_response(
                    StatusCode::NOT_FOUND,
                    "Post not found or already deleted",
                    false,
                );
            }
        }
        Err(err) => {
            eprintln!("❌ [ScyllaDB update posts_by_id {post_uuid}]: {err}");
            return toast_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Failed to update post",
                true,
            );
        }
    }

    // Execute LWT (IF EXISTS) on posts as well so concurrent delete_post can never leave a resurrected row
    match state
        .session
        .execute_unpaged(
            &state.stmt_update_post,
            (&title, &content, bucket, cql_ts, post_uuid),
        )
        .await
    {
        Ok(res) => {
            if !is_lwt_applied(res) {
                return toast_error_response(
                    StatusCode::NOT_FOUND,
                    "Post not found or already deleted",
                    false,
                );
            }
        }
        Err(err) => {
            eprintln!("❌ [ScyllaDB update posts {post_uuid}]: {err}");
            // Compensating LWT rollback in posts_by_id to preserve cross-table consistency
            let _ = state
                .session
                .execute_unpaged(
                    &state.stmt_update_post_by_id,
                    (&existing_post.title, &existing_post.content, post_uuid),
                )
                .await;

            return toast_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Failed to update post",
                true,
            );
        }
    }

    let updated_post = match fetch_post_by_uuid(&state, post_uuid).await {
        PostLookup::Found(_, post) => post,
        PostLookup::NotFound | PostLookup::Unavailable => Post {
            id_hex: existing_post.id_hex,
            title,
            content,
            created_at: existing_post.created_at,
            date_str: existing_post.date_str,
        },
    };

    let post_url = format!("/posts/{}", updated_post.id_hex);
    let seo_title = if updated_post.title.is_empty() {
        format!(
            "{} — Add Post (SAHAR)",
            truncate(&updated_post.content, 40).replace(['\n', '\r', '\t'], " ")
        )
    } else {
        format!("{} — Add Post (SAHAR)", updated_post.title)
    };
    let safe_title = html_escape_title(&seo_title);

    let detail_html = PostDetailTemplate {
        post: Some(updated_post),
    }
    .render()
    .map_err(|e| internal_error("TEMPLATE post_detail", e))?;

    let toast_html = ToastTemplate {
        message: "Post updated successfully".to_string(),
        is_success: true,
    }
    .render()
    .map_err(|e| internal_error("TEMPLATE toast", e))?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header(("HX-Push-Url", post_url))
        .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
        .body(format!(
            "<title>{safe_title}</title>\n{detail_html}\n{toast_html}"
        )))
}

pub async fn delete_post(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<DeleteQuery>,
) -> Result<HttpResponse, Error> {
    validate_origin(&req)?;

    let raw_id = path.into_inner();
    let Ok(post_uuid) = Uuid::parse_str(&raw_id) else {
        return toast_error_response(StatusCode::BAD_REQUEST, "Invalid post ID", false);
    };

    // Fetch authoritative bucket and created_at from DB — never trust client-supplied values
    let (bucket, existing_post) = match fetch_post_by_uuid(&state, post_uuid).await {
        PostLookup::Found(bucket, post) => (bucket, post),
        PostLookup::NotFound => {
            return toast_error_response(
                StatusCode::NOT_FOUND,
                "Post not found or already deleted",
                false,
            );
        }
        PostLookup::Unavailable => {
            return toast_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Failed to delete post",
                true,
            );
        }
    };

    // Atomically claim deletion via LWT (IF EXISTS) to prevent concurrent double-decrement
    match state
        .session
        .execute_unpaged(&state.stmt_delete_post_by_id, (post_uuid,))
        .await
    {
        Ok(res) => {
            if !is_lwt_applied(res) {
                return toast_error_response(
                    StatusCode::NOT_FOUND,
                    "Post not found or already deleted",
                    false,
                );
            }
        }
        Err(err) => {
            eprintln!("❌ [ScyllaDB delete posts_by_id {post_uuid}]: {err}");
            return toast_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Failed to delete post",
                true,
            );
        }
    }

    let del_main = state
        .session
        .execute_unpaged(
            &state.stmt_delete_post,
            (bucket, CqlTimestamp(existing_post.created_at), post_uuid),
        )
        .await;

    if let Err(err) = del_main {
        eprintln!("❌ [ScyllaDB delete posts {post_uuid}]: {err}");
        // Compensating LWT rollback (IF NOT EXISTS) so Paxos timestamp wins over the preceding LWT delete
        let _ = state
            .session
            .execute_unpaged(
                &state.stmt_restore_post_by_id,
                (
                    post_uuid,
                    bucket,
                    CqlTimestamp(existing_post.created_at),
                    &existing_post.title,
                    &existing_post.content,
                ),
            )
            .await;

        return toast_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Failed to delete post",
            true,
        );
    }

    // Decrement counter only when LWT confirmed deletion and current counter is > 0 (never go negative)
    if let Some(total) = get_total_posts(&state).await
        && total > 0
        && let Err(err) = state
            .session
            .execute_unpaged(&state.stmt_dec_counter, (1_i32,))
            .await
    {
        eprintln!("❌ [ScyllaDB decrement counter]: {err}");
    }

    let toast_html = ToastTemplate {
        message: "Post deleted successfully".to_string(),
        is_success: true,
    }
    .render()
    .map_err(|e| internal_error("TEMPLATE toast", e))?;

    if query.redirect.as_deref() == Some("blog") {
        let items_html = match render_blog_feed_html(&state, None).await {
            Ok(html) => html,
            Err(resp) => return Ok(*resp),
        };
        let blog_html = BlogTemplate {
            posts_html: items_html,
        }
        .render()
        .map_err(|e| internal_error("TEMPLATE blog", e))?;

        return Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("HX-Push-Url", "/blog"))
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .insert_header(("Vary", VARY_HTMX_HEADERS))
            .body(format!(
                "<title>Blog — Add Post (SAHAR)</title>\n{blog_html}\n{toast_html}"
            )));
    }

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
        .body(toast_html))
}