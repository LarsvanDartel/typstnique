use leptos::prelude::*;
use leptos_router::components::A;

/// Simple landing page.
#[component]
pub fn HomePage() -> impl IntoView {
    view! {
        <section class="hero">
            <h1 class="hero-title">"Typstnique"</h1>
            <p class="hero-tagline">
                "A typesetting speed game for "
                <a href="https://typst.app" target="_blank" rel="noreferrer">"Typst"</a>
                ". How many formulas can you recreate in three minutes?"
            </p>
            <div class="cta-wrap">
                <A href="/play">"▶ Play"</A>
            </div>
            <nav class="hero-links">
                <A href="/practice">"Practice mode"</A>
                <A href="/problems">"Browse problems"</A>
                <A href="/leaderboard">"Leaderboard"</A>
            </nav>
            <div class="hero-how panel">
                <h3>"How it works"</h3>
                <ol>
                    <li>"You're shown a rendered formula — the "<b>"goal"</b>"."</li>
                    <li>"Type Typst to recreate it; the live preview updates as you go."</li>
                    <li>"When your render matches the goal, it's accepted and you advance."</li>
                    <li>"Score as much as you can before the timer runs out."</li>
                </ol>
            </div>
        </section>
    }
}
