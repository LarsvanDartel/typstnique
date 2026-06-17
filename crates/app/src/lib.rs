//! Typstnique — Leptos application (shared between server SSR and WASM hydrate).

// Leptos components and view-builders are consumed by the framework/macros,
// not by checking their return value.
#![allow(clippy::must_use_candidate)]

pub mod game;
pub mod pages;
pub mod problems;
pub mod server_fns;

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::path;

use pages::{HomePage, LeaderboardPage, PlayPage, PracticePage, ProblemsPage};

/// The HTML document shell rendered by the server (and used for hydration).
// `leptos_axum` requires this signature (owned `LeptosOptions`, by value).
#[allow(clippy::needless_pass_by_value)]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme="dark">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                // Apply the saved theme before first paint to avoid a flash.
                <script src="/theme-init.js"></script>
                <AutoReload options=options.clone()/>
                <HydrationScripts options=options.clone()/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

/// Root application component.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // Theme state, persisted to localStorage and reflected on <html data-theme>.
    // SSR/initial render uses "dark" (matches the shell) to keep hydration in
    // sync; the head script has already applied the *real* theme before paint.
    let theme = RwSignal::new("dark".to_string());

    Effect::new(move |prev: Option<()>| {
        // On the first client run, adopt the saved theme BEFORE touching
        // `data-theme`, so we never overwrite the head script's value with the
        // default and flash the wrong theme during hydration.
        if prev.is_none() {
            if let Some(saved) = window()
                .local_storage()
                .ok()
                .flatten()
                .and_then(|s| s.get_item("theme").ok().flatten())
            {
                theme.set(saved);
            }
        }
        let t = theme.get();
        if let Some(el) = document().document_element() {
            let _ = el.set_attribute("data-theme", &t);
        }
        if let Some(storage) = window().local_storage().ok().flatten() {
            let _ = storage.set_item("theme", &t);
        }
    });

    let toggle_theme = move |_| {
        theme.update(|t| *t = if t == "dark" { "light".into() } else { "dark".into() });
    };

    view! {
        <Stylesheet id="leptos" href="/pkg/typstnique.css"/>
        <Title text="Typstnique"/>
        <Router>
            <nav class="nav">
                <A href="/">"Typstnique"</A>
                <span class="nav-spacer"></span>
                <A href="/play">"Play"</A>
                <A href="/practice">"Practice"</A>
                <A href="/problems">"Problems"</A>
                <A href="/leaderboard">"Leaderboard"</A>
                <button class="theme-toggle" on:click=toggle_theme title="Toggle light/dark">
                    {move || if theme.get() == "dark" { "☀ Light" } else { "🌙 Dark" }}
                </button>
            </nav>
            <main>
                <Routes fallback=|| view! { <p>"Not found"</p> }>
                    <Route path=path!("") view=HomePage/>
                    <Route path=path!("/play") view=PlayPage/>
                    <Route path=path!("/practice") view=PracticePage/>
                    <Route path=path!("/problems") view=ProblemsPage/>
                    <Route path=path!("/leaderboard") view=LeaderboardPage/>
                </Routes>
            </main>
        </Router>
    }
}

/// WASM entry point: hydrate the server-rendered HTML.
#[cfg(feature = "hydrate")]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    // Log to the browser console at Debug.
    let _ = console_log::init_with_level(log::Level::Debug);
    leptos::mount::hydrate_body(App);
}
