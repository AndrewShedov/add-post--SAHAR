use askama::Template;
use serde::Deserialize;
use uuid::Uuid;

pub struct Post {
    pub id: Uuid,
    pub id_hex: String,
    pub title: String,
    pub content: String,
    pub created_at: i64,
    pub date_str: String,
}

#[derive(Deserialize)]
pub struct PostForm {
    pub title: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct PagingQuery {
    pub before: Option<i64>,
}

#[derive(Deserialize)]
pub struct EditQuery {
    pub from: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteQuery {
    pub redirect: Option<String>,
}

#[derive(Template)]
#[template(path = "base.html")]
pub struct BaseTemplate {
    pub page_title: String,
    pub page_description: String,
    pub content_html: String,
    pub current_path: String,
}

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomeTemplate {
    pub total_posts: i64,
}

#[derive(Template)]
#[template(path = "blog.html")]
pub struct BlogTemplate {
    pub posts_html: String,
}

#[derive(Template)]
#[template(path = "post_items.html")]
pub struct PostItemsTemplate {
    pub posts: Vec<Post>,
    pub next_timestamp: Option<i64>,
}

#[derive(Template)]
#[template(path = "post_detail.html")]
pub struct PostDetailTemplate {
    pub post: Option<Post>,
}

#[derive(Template)]
#[template(path = "post_edit.html")]
pub struct PostEditTemplate {
    pub post: Option<Post>,
    pub back_url: String,
}

#[derive(Template)]
#[template(path = "toast.html")]
pub struct ToastTemplate {
    pub message: String,
    pub is_success: bool,
}
