// Artificial delay in milliseconds to visually inspect and record UI loaders.
// Set to 0 for instant speed, or e.g. 800 to see "Loading posts... ⏳" and "Loading more posts... ⏳".

// Loader Delay
// pub const LOADER_DELAY_MS: u64 = 800;
// /Loader Delay

mod db;
mod handlers;
mod models;
mod security;

use actix_files::Files;
use actix_web::{middleware, web, App, HttpServer};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(db::init_db().await);

    println!("🚀 SAHAR server is running at http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .app_data(web::FormConfig::default().limit(65536))
            .wrap(
                middleware::DefaultHeaders::new()
                    .add(("X-Frame-Options", "DENY"))
                    .add(("X-Content-Type-Options", "nosniff"))
                    .add((
                        "Content-Security-Policy",
                        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self';",
                    ))
                    .add((
                        "Strict-Transport-Security",
                        "max-age=31536000; includeSubDomains; preload",
                    ))
                    .add(("Referrer-Policy", "strict-origin-when-cross-origin")),
            )
            .service(Files::new("/static", "./static"))
            .route("/", web::get().to(handlers::index))
            .route("/blog", web::get().to(handlers::blog))
            .route("/posts/{id}", web::get().to(handlers::view_post))
            .route("/posts/{id}/edit", web::get().to(handlers::edit_post_page))
            .route("/api/posts", web::post().to(handlers::create_post))
            .route("/api/posts/{id}/edit", web::post().to(handlers::update_post))
            .route(
                "/api/posts/{created_at}/{id}",
                web::delete().to(handlers::delete_post),
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}