use crate::db::{fetch_post_by_uuid, get_total_posts, AppState};
use crate::models::{
    BlogTemplate, DeleteQuery, EditQuery, HomeTemplate, PagingQuery, Post, PostDetailTemplate,
    PostEditTemplate, PostForm, PostItemsTemplate, ToastTemplate,
};
use crate::security::{
    html_escape_title, is_htmx_partial_request, render_not_found, render_page, truncate,
    validate_origin,
};
use actix_web::{error, web, Error, HttpRequest, HttpResponse};
use askama::Template;
use chrono::{DateTime, Utc};
use scylla::value::CqlTimestamp;
use uuid::Uuid;

async fn render_blog_feed_html(
    state: &web::Data<AppState>,
    before: Option<i64>,
) -> Result<String, Error> {
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

    if let Ok(res_data) = res {
        if let Ok(rows) = res_data.into_rows_result() {
            if let Ok(row_iter) = rows.rows::<(CqlTimestamp, Uuid, String, String)>() {
                for row_res in row_iter {
                    if let Ok((ts, id, title, content)) = row_res {
                        let dt = DateTime::<Utc>::from_timestamp_millis(ts.0).unwrap_or_default();
                        posts.push(Post {
                            id,
                            id_hex: id.simple().to_string(),
                            // Limit preview card title to 100 chars and post text to 730 chars in the /blog feed
                            title: truncate(&title, 100),
                            content: truncate(&content, 730),
                            created_at: ts.0,
                            date_str: dt.format("%d %b %Y, %H:%M").to_string(),
                        });
                    }
                }
            }
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
    .map_err(error::ErrorInternalServerError)
}

pub async fn index(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let total_posts = get_total_posts(&state).await;
    let partial = HomeTemplate { total_posts }
        .render()
        .map_err(error::ErrorInternalServerError)?;
    render_page(
        &req,
        partial,
        "/",
        "Add Post (SAHAR)",
        "Create a new post in the SAHAR stack prototype.",
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

    let items_html = render_blog_feed_html(&state, query.before).await?;

    if is_htmx_partial_request(&req) && query.before.is_some() {
        return Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .body(items_html));
    }

    let partial = BlogTemplate {
        posts_html: items_html,
    }
    .render()
    .map_err(error::ErrorInternalServerError)?;

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

    let Some((_bucket, post)) = fetch_post_by_uuid(&state, post_uuid).await else {
        return render_not_found(&req, &current_path);
    };

    let canonical_path = format!("/posts/{}", post.id_hex);
    let seo_title = if post.title.is_empty() {
        format!("{} — Add Post (SAHAR)", truncate(&post.content, 40))
    } else {
        format!("{} — Add Post (SAHAR)", post.title)
    };
    let seo_desc = truncate(&post.content, 150);

    let partial = PostDetailTemplate { post: Some(post) }
        .render()
        .map_err(error::ErrorInternalServerError)?;

    render_page(&req, partial, &canonical_path, &seo_title, &seo_desc)
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

    let Some((_bucket, post)) = fetch_post_by_uuid(&state, post_uuid).await else {
        return render_not_found(&req, &current_path);
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
    .map_err(error::ErrorInternalServerError)?;

    render_page(
        &req,
        partial,
        &canonical_path,
        "Edit post — Add Post (SAHAR)",
        "Edit post in the SAHAR stack prototype.",
    )
}

pub async fn create_post(
    req: HttpRequest,
    state: web::Data<AppState>,
    form: web::Form<PostForm>,
) -> Result<HttpResponse, Error> {
    validate_origin(&req)?;

    let title = form.title.trim().chars().take(150).collect::<String>();
    let content = form.content.trim().chars().take(3000).collect::<String>();

    if content.is_empty() {
        return Ok(HttpResponse::BadRequest().body("Content is required"));
    }

    let post_id = Uuid::new_v4();
    let now_ms = Utc::now().timestamp_millis();
    let cql_ts = CqlTimestamp(now_ms);

    let res_main = state
        .session
        .execute_unpaged(
            &state.stmt_insert_post,
            (1_i32, cql_ts, post_id, &title, &content),
        )
        .await;

    let res_by_id = state
        .session
        .execute_unpaged(
            &state.stmt_insert_post_by_id,
            (post_id, 1_i32, cql_ts, &title, &content),
        )
        .await;

    let (msg, is_success) = if res_main.is_ok() && res_by_id.is_ok() {
        let _ = state
            .session
            .execute_unpaged(&state.stmt_inc_counter, (1_i32,))
            .await;
        ("Post added successfully", true)
    } else {
        ("Failed to add post", false)
    };

    let total_posts = get_total_posts(&state).await;

    let form_html = HomeTemplate { total_posts }
        .render()
        .map_err(error::ErrorInternalServerError)?;
    let toast_html = ToastTemplate {
        message: msg.to_string(),
        is_success,
    }
    .render()
    .map_err(error::ErrorInternalServerError)?;

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
        return Ok(HttpResponse::BadRequest().body("Invalid post ID"));
    };

    let title = form.title.trim().chars().take(150).collect::<String>();
    let content = form.content.trim().chars().take(3000).collect::<String>();

    if content.is_empty() {
        return Ok(HttpResponse::BadRequest().body("Content is required"));
    }

    let Some((bucket, existing_post)) = fetch_post_by_uuid(&state, post_uuid).await else {
        return render_not_found(&req, &format!("/posts/{raw_id}"));
    };

    let cql_ts = CqlTimestamp(existing_post.created_at);

    let res_main = state
        .session
        .execute_unpaged(
            &state.stmt_update_post,
            (&title, &content, bucket, cql_ts, post_uuid),
        )
        .await;

    let res_by_id = state
        .session
        .execute_unpaged(&state.stmt_update_post_by_id, (&title, &content, post_uuid))
        .await;

    let (msg, is_success) = if res_main.is_ok() && res_by_id.is_ok() {
        ("Post updated successfully", true)
    } else {
        ("Failed to update post", false)
    };

    let updated_post = fetch_post_by_uuid(&state, post_uuid)
        .await
        .map(|(_, p)| p)
        .unwrap_or(existing_post);

    let post_url = format!("/posts/{}", updated_post.id_hex);
    let seo_title = if updated_post.title.is_empty() {
        format!("{} — Add Post (SAHAR)", truncate(&updated_post.content, 40))
    } else {
        format!("{} — Add Post (SAHAR)", updated_post.title)
    };
    let safe_title = html_escape_title(&seo_title);

    let detail_html = PostDetailTemplate {
        post: Some(updated_post),
    }
    .render()
    .map_err(error::ErrorInternalServerError)?;

    let toast_html = ToastTemplate {
        message: msg.to_string(),
        is_success,
    }
    .render()
    .map_err(error::ErrorInternalServerError)?;

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
    path: web::Path<(i64, Uuid)>,
    query: web::Query<DeleteQuery>,
) -> Result<HttpResponse, Error> {
    validate_origin(&req)?;
    let (created_at, post_id) = path.into_inner();

    // Check if the post actually exists before deleting and decrementing the counter
    let post_exists = fetch_post_by_uuid(&state, post_id).await.is_some();

    let del_main = state
        .session
        .execute_unpaged(
            &state.stmt_delete_post,
            (1_i32, CqlTimestamp(created_at), post_id),
        )
        .await;

    let _ = state
        .session
        .execute_unpaged(&state.stmt_delete_post_by_id, (post_id,))
        .await;

    if del_main.is_ok() {
        // Decrement counter only if the post existed and current counter is > 0 (never go negative)
        if post_exists && get_total_posts(&state).await > 0 {
            let _ = state
                .session
                .execute_unpaged(&state.stmt_dec_counter, (1_i32,))
                .await;
        }

        let toast_html = ToastTemplate {
            message: "Post deleted successfully".to_string(),
            is_success: true,
        }
        .render()
        .map_err(error::ErrorInternalServerError)?;

        if query.redirect.as_deref() == Some("blog") {
            let items_html = render_blog_feed_html(&state, None).await?;
            let blog_html = BlogTemplate {
                posts_html: items_html,
            }
            .render()
            .map_err(error::ErrorInternalServerError)?;

            return Ok(HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .insert_header(("HX-Push-Url", "/blog"))
                .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
                .body(format!(
                    "<title>Blog — Add Post (SAHAR)</title>\n{blog_html}\n{toast_html}"
                )));
        }

          Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-store, no-cache, must-revalidate"))
            .body(toast_html))
    } else {
        Ok(HttpResponse::InternalServerError().body("Failed to delete"))
    }
}