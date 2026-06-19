//! Axum server: serves the Leptos app and the leaderboard `SQLite` database.

use std::str::FromStr;

use app::{shell, App};
use axum::Router;
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Structured logging to stdout. `RUST_LOG` (e.g. `RUST_LOG=debug`) overrides;
    // default is Info with noisy crates quieted. Each line carries the structured
    // fields (request_id, session, …) the server functions attach to their events.
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,hyper=warn,sqlx=warn,tower=warn")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let conf = get_configuration(None).expect("read leptos configuration");
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;

    // Open (creating if needed) the leaderboard database and run migrations.
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:typstnique.db".to_string());
    let connect_options = SqliteConnectOptions::from_str(&db_url)
        .expect("valid DATABASE_URL")
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options)
        .await
        .expect("connect to database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    // In-memory store for active game sessions (server-authoritative scoring).
    let sessions = app::server_fns::ssr::new_sessions();

    // Background task: remove sessions that have outlived SESSION_TTL_SECONDS.
    // Runs every ~120 s — well within the TTL — so abandoned games don't leak.
    {
        let sessions = sessions.clone();
        tokio::spawn(async move {
            let interval = std::time::Duration::from_mins(2);
            loop {
                tokio::time::sleep(interval).await;
                let reaped = app::server_fns::ssr::reap_stale(&sessions);
                if reaped > 0 {
                    tracing::debug!(reaped, "session reaper");
                }
            }
        });
    }

    let routes = generate_route_list(App);

    let app = Router::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                // Inject shared state so server functions can `use_context` it.
                let pool = pool.clone();
                let sessions = sessions.clone();
                move || {
                    provide_context(pool.clone());
                    provide_context(sessions.clone());
                }
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    tracing::info!("typstnique listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("bind listener");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("serve");
}
