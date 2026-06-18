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

/// Public repository, linked from the nav.
const REPO_URL: &str = "https://github.com/LarsvanDartel/typstnique";

/// GitHub mark (inherits `currentColor`).
const GITHUB_ICON: &str = r#"<svg viewBox="0 0 16 16" width="20" height="20" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z"></path></svg>"#;

/// Feather "sun" — shown in dark mode (click to switch to light).
const SUN_ICON: &str = r#"<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"></path></svg>"#;

/// Feather "moon" — shown in light mode (click to switch to dark).
const MOON_ICON: &str = r#"<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path></svg>"#;

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
        theme.update(|t| {
            *t = if t == "dark" {
                "light".into()
            } else {
                "dark".into()
            }
        });
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
                <a
                    class="nav-icon"
                    href=REPO_URL
                    target="_blank"
                    rel="noreferrer"
                    title="View on GitHub"
                    aria-label="View on GitHub"
                    inner_html=GITHUB_ICON
                ></a>
                <button
                    class="theme-toggle"
                    on:click=toggle_theme
                    title="Toggle light/dark"
                    aria-label="Toggle light/dark theme"
                    inner_html=move || if theme.get() == "dark" { SUN_ICON } else { MOON_ICON }
                ></button>
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
