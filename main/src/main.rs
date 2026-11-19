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
use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use std::time::Duration;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Fail-Fast validation of runtime origin & canonical URL config before binding sockets
    security::init_security_config();

    let app_state = web::Data::new(db::init_db().await);
    let bind_addr =
        std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

    println!("🚀 SAHAR server is running at http://{bind_addr}");

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            // DoS, Memory Exhaustion & CWE-209 Protection:
            // Strictly limits incoming form payloads to 64 KB and suppresses raw Rust/Serde parser errors.
            .app_data(
                web::FormConfig::default()
                    .limit(65536)
                    .error_handler(|err, _req| {
                        eprintln!("⚠️ [FORM ERROR]: {err}");
                        actix_web::error::InternalError::from_response(
                            err,
                            HttpResponse::BadRequest().body("Bad Request"),
                        )
                        .into()
                    }),
            )
            .app_data(
                web::QueryConfig::default().error_handler(|err, _req| {
                    eprintln!("⚠️ [QUERY ERROR]: {err}");
                    actix_web::error::InternalError::from_response(
                        err,
                        HttpResponse::BadRequest().body("Bad Request"),
                    )
                    .into()
                }),
            )
            .app_data(
                web::PathConfig::default().error_handler(|err, _req| {
                    eprintln!("⚠️ [PATH ERROR]: {err}");
                    actix_web::error::InternalError::from_response(
                        err,
                        HttpResponse::BadRequest().body("Bad Request"),
                    )
                    .into()
                }),
            )
            // Strict HTTP Security Headers Middleware (Zero 'unsafe-inline' tolerance)
            .wrap(
                middleware::DefaultHeaders::new()
                    .add(("X-Frame-Options", "DENY"))
                    .add(("X-Content-Type-Options", "nosniff"))
                    .add((
                        "Content-Security-Policy",
                        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none';",
                    ))
                    .add((
                        "Strict-Transport-Security",
                        "max-age=31536000; includeSubDomains; preload",
                    ))
                    .add(("Referrer-Policy", "strict-origin-when-cross-origin"))
                    .add((
                        "Permissions-Policy",
                        "geolocation=(), microphone=(), camera=(), payment=(), usb=(), interest-cohort=()",
                    ))
                    .add(("Cross-Origin-Opener-Policy", "same-origin"))
                    .add(("Cross-Origin-Resource-Policy", "same-origin"))
                    .add(("X-Permitted-Cross-Domain-Policies", "none")),
            )
            .service(Files::new("/static", "./static"))
            .route("/", web::get().to(handlers::index))
            .route("/blog", web::get().to(handlers::blog))
            .route("/posts/{id}", web::get().to(handlers::view_post))
            .route("/posts/{id}/edit", web::get().to(handlers::edit_post_page))
            .route("/api/posts", web::post().to(handlers::create_post))
            .route("/api/posts/{id}/edit", web::post().to(handlers::update_post))
            .route("/api/posts/{id}", web::delete().to(handlers::delete_post))
    })
    .client_request_timeout(Duration::from_secs(5))
    .keep_alive(Duration::from_secs(30))
    .bind(&bind_addr)?
    .run()
    .await
}