use crate::models::Post;
use actix_web::web;
use chrono::{DateTime, Utc};
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use scylla::statement::prepared::PreparedStatement;
use scylla::value::{Counter, CqlTimestamp};
use std::sync::Arc;
use uuid::Uuid;

pub struct AppState {
    pub session: Arc<Session>,
    pub stmt_insert_post: PreparedStatement,
    pub stmt_insert_post_by_id: PreparedStatement,
    pub stmt_update_post: PreparedStatement,
    pub stmt_update_post_by_id: PreparedStatement,
    pub stmt_inc_counter: PreparedStatement,
    pub stmt_dec_counter: PreparedStatement,
    pub stmt_get_counter: PreparedStatement,
    pub stmt_get_posts: PreparedStatement,
    pub stmt_get_posts_paged: PreparedStatement,
    pub stmt_get_post_by_id: PreparedStatement,
    pub stmt_delete_post: PreparedStatement,
    pub stmt_delete_post_by_id: PreparedStatement,
}

pub async fn init_db() -> AppState {
    // 1. Establish connection to the local ScyllaDB node
    let session = SessionBuilder::new()
        .known_node("127.0.0.1:9042")
        .build()
        .await
        .expect("Failed to connect to ScyllaDB");

    // 2. Automatically create the keyspace and tables if they do not exist yet.

    // NOTE (Best Practice & Security):
    // Auto-initialization is enabled here for zero-config prototype reproducibility.
    // In a production environment, DDL schema migrations (CREATE/ALTER/DROP) must be
    // executed separately via a dedicated migration tool, and the web server must
    // connect using a restricted DB role (SELECT/MODIFY only — Principle of Least Privilege).
    initialize_schema(&session).await;

    // 3. Switch the active session to the newly created (or existing) keyspace
    session
        .use_keyspace("sahar_prototype_add_post", false)
        .await
        .expect("Failed to select keyspace");

    sync_on_startup(&session).await;

    let stmt_insert_post = session
        .prepare(
            "INSERT INTO posts (bucket, created_at, id, title, content) VALUES (?, ?, ?, ?, ?)",
        )
        .await
        .unwrap();
    let stmt_insert_post_by_id = session
        .prepare("INSERT INTO posts_by_id (id, bucket, created_at, title, content) VALUES (?, ?, ?, ?, ?)")
        .await
        .unwrap();
    let stmt_update_post = session
        .prepare("UPDATE posts SET title = ?, content = ? WHERE bucket = ? AND created_at = ? AND id = ?")
        .await
        .unwrap();
    let stmt_update_post_by_id = session
        .prepare("UPDATE posts_by_id SET title = ?, content = ? WHERE id = ?")
        .await
        .unwrap();
    let stmt_inc_counter = session
        .prepare("UPDATE post_counts SET total = total + 1 WHERE bucket = ?")
        .await
        .unwrap();
    let stmt_dec_counter = session
        .prepare("UPDATE post_counts SET total = total - 1 WHERE bucket = ?")
        .await
        .unwrap();
    let stmt_get_counter = session
        .prepare("SELECT total FROM post_counts WHERE bucket = ?")
        .await
        .unwrap();
    let stmt_get_posts = session
        .prepare("SELECT created_at, id, title, content FROM posts WHERE bucket = ? LIMIT 5")
        .await
        .unwrap();
    let stmt_get_posts_paged = session
        .prepare("SELECT created_at, id, title, content FROM posts WHERE bucket = ? AND created_at < ? LIMIT 5")
        .await
        .unwrap();
    let stmt_get_post_by_id = session
        .prepare("SELECT bucket, created_at, title, content FROM posts_by_id WHERE id = ?")
        .await
        .unwrap();
    let stmt_delete_post = session
        .prepare("DELETE FROM posts WHERE bucket = ? AND created_at = ? AND id = ?")
        .await
        .unwrap();
    let stmt_delete_post_by_id = session
        .prepare("DELETE FROM posts_by_id WHERE id = ?")
        .await
        .unwrap();

    println!("✅ Keyspace 'sahar_prototype_add_post' and tables are ready.");

    AppState {
        session: Arc::new(session),
        stmt_insert_post,
        stmt_insert_post_by_id,
        stmt_update_post,
        stmt_update_post_by_id,
        stmt_inc_counter,
        stmt_dec_counter,
        stmt_get_counter,
        stmt_get_posts,
        stmt_get_posts_paged,
        stmt_get_post_by_id,
        stmt_delete_post,
        stmt_delete_post_by_id,
    }
}

// Idempotently creates the `sahar_prototype_add_post` keyspace and required tables.
async fn initialize_schema(session: &Session) {
    println!("🧪 Initializing ScyllaDB schema...");

    session
        .query_unpaged(
            "CREATE KEYSPACE IF NOT EXISTS sahar_prototype_add_post WITH REPLICATION = {'class': 'NetworkTopologyStrategy', 'replication_factor': 1}",
            &[],
        )
        .await
        .expect("Failed to create keyspace");

    session
        .query_unpaged(
            "CREATE TABLE IF NOT EXISTS sahar_prototype_add_post.posts (
                bucket int,
                created_at timestamp,
                id uuid,
                title text,
                content text,
                PRIMARY KEY (bucket, created_at, id)
            ) WITH CLUSTERING ORDER BY (created_at DESC)",
            &[],
        )
        .await
        .expect("Failed to create posts table");

    session
        .query_unpaged(
            "CREATE TABLE IF NOT EXISTS sahar_prototype_add_post.posts_by_id (
                id uuid PRIMARY KEY,
                bucket int,
                created_at timestamp,
                title text,
                content text
            )",
            &[],
        )
        .await
        .expect("Failed to create posts_by_id table");

    session
        .query_unpaged(
            "CREATE TABLE IF NOT EXISTS sahar_prototype_add_post.post_counts (
                bucket int PRIMARY KEY,
                total counter
            )",
            &[],
        )
        .await
        .expect("Failed to create post_counts table");
}

async fn sync_on_startup(session: &Session) {
    let mut actual_count: i64 = 0;

    if let Ok(res) = session
        .query_unpaged(
            "SELECT bucket, created_at, id, title, content FROM posts WHERE bucket = 1",
            &[],
        )
        .await
    {
        if let Ok(rows) = res.into_rows_result() {
            if let Ok(iter) = rows.rows::<(i32, CqlTimestamp, Uuid, String, String)>() {
                for row in iter.flatten() {
                    actual_count += 1;
                    let (bucket, created_at, id, title, content) = row;
                    let _ = session
                        .query_unpaged(
                            "INSERT INTO posts_by_id (id, bucket, created_at, title, content) VALUES (?, ?, ?, ?, ?)",
                            (id, bucket, created_at, title, content),
                        )
                        .await;
                }
            }
        }
    }

    let current_counter: i64 = match session
        .query_unpaged("SELECT total FROM post_counts WHERE bucket = 1", &[])
        .await
    {
        Ok(res) => res
            .into_rows_result()
            .ok()
            .and_then(|rows| rows.maybe_first_row::<(Option<Counter>,)>().ok().flatten())
            .and_then(|r| r.0)
            .map(|c| c.0)
            .unwrap_or(0),
        Err(_) => 0,
    };

    let diff = actual_count - current_counter;
    if diff != 0 {
        let _ = session
            .query_unpaged(
                "UPDATE post_counts SET total = total + ? WHERE bucket = 1",
                (Counter(diff),),
            )
            .await;
    }
}

pub async fn get_total_posts(state: &web::Data<AppState>) -> i64 {
    if let Ok(res) = state
        .session
        .execute_unpaged(&state.stmt_get_counter, (1_i32,))
        .await
    {
        if let Ok(rows) = res.into_rows_result() {
            if let Ok(Some((Some(counter),))) = rows.maybe_first_row::<(Option<Counter>,)>() {
                return counter.0.max(0);
            }
        }
    }
    0
}

pub async fn fetch_post_by_uuid(
    state: &web::Data<AppState>,
    post_uuid: Uuid,
) -> Option<(i32, Post)> {
    if let Ok(res) = state
        .session
        .execute_unpaged(&state.stmt_get_post_by_id, (post_uuid,))
        .await
    {
        if let Ok(rows) = res.into_rows_result() {
            if let Ok(Some((bucket, ts, title, content))) =
                rows.maybe_first_row::<(i32, CqlTimestamp, String, String)>()
            {
                let dt = DateTime::<Utc>::from_timestamp_millis(ts.0).unwrap_or_default();
                let post = Post {
                    id: post_uuid,
                    id_hex: post_uuid.simple().to_string(),
                    title,
                    content,
                    created_at: ts.0,
                    date_str: dt.format("%d %b %Y, %H:%M").to_string(),
                };
                return Some((bucket, post));
            }
        }
    }
    None
}
