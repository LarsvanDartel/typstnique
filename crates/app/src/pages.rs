//! Route page components.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::game::GameBoard;
use crate::problems::load_problems;
use crate::server_fns::top_scores;

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

/// Timed 3-minute game (server-scored, feeds the leaderboard).
#[component]
pub fn PlayPage() -> impl IntoView {
    view! {
        <h1>"Typstnique"</h1>
        <p>"Recreate as many formulas as you can in three minutes."</p>
        <GameBoard timed=true server_scored=true/>
    }
}

/// Open-ended practice (no timer).
#[component]
pub fn PracticePage() -> impl IntoView {
    let problems = load_problems();
    view! {
        <h1>"Practice"</h1>
        <p>"Work through the problems at your own pace."</p>
        <GameBoard problems=problems/>
    }
}

/// One problem in the gallery: title, difficulty, and a progressively-filled goal.
#[component]
fn ProblemCard(title: String, stars: String, points: u32, svg: RwSignal<String>) -> impl IntoView {
    view! {
        <div class="panel">
            <div class="problem-title">{title}</div>
            <div class="difficulty">{stars} " · " {points} " pts"</div>
            <div class="target" inner_html=move || svg.get()></div>
        </div>
    }
}

/// Browsable list of all problems. Targets are rendered progressively on the
/// client — one per animation frame — so the page never blocks compiling every
/// formula at once. Panels (title/difficulty) appear immediately; each goal
/// pops in as it's rendered.
#[component]
pub fn ProblemsPage() -> impl IntoView {
    let problems = StoredValue::new(load_problems());
    let count = problems.with_value(Vec::len);
    // One SVG signal per problem, filled in progressively.
    let svgs = StoredValue::new(
        (0..count)
            .map(|_| RwSignal::new(String::new()))
            .collect::<Vec<_>>(),
    );

    // Render the next target each animation frame, yielding to the browser in
    // between (so it stays responsive and they appear one by one). Effects run
    // only on the client, so SSR/hydration stay cheap.
    let next = RwSignal::new(0usize);
    Effect::new(move |_| {
        let i = next.get();
        if i >= count {
            return;
        }
        let svg = problems
            .with_value(|p| typst_engine::render_svg(&p[i].source, p[i].kind).unwrap_or_default());
        svgs.with_value(|s| s[i].set(svg));
        request_animation_frame(move || next.update(|n| *n += 1));
    });

    view! {
        <h1>"Problems"</h1>
        <div class="problem-grid">
            {(0..count)
                .map(|i| {
                    let (title, stars, points) = problems.with_value(|p| {
                        (
                            p[i].title.clone(),
                            "★".repeat(usize::try_from(p[i].difficulty()).unwrap_or(0)),
                            p[i].points(),
                        )
                    });
                    let svg = svgs.with_value(|s| s[i]);
                    view! { <ProblemCard title=title stars=stars points=points svg=svg/> }
                })
                .collect_view()}
        </div>
    }
}

/// Global leaderboard, fetched from the server.
#[component]
pub fn LeaderboardPage() -> impl IntoView {
    let scores = Resource::new(|| (), |()| async move { top_scores().await });
    view! {
        <h1>"Leaderboard"</h1>
        <Suspense fallback=move || view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                match scores.await {
                    Ok(list) if list.is_empty() => {
                        view! { <p>"No scores yet — be the first!"</p> }.into_any()
                    }
                    Ok(list) => view! {
                        <table>
                            <thead>
                                <tr>
                                    <th>"#"</th>
                                    <th>"Name"</th>
                                    <th>"Score"</th>
                                    <th>"Solved"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {list
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, e)| view! {
                                        <tr>
                                            <td>{i + 1}</td>
                                            <td>{e.name}</td>
                                            <td>{e.score}</td>
                                            <td>{e.problems_solved}</td>
                                        </tr>
                                    })
                                    .collect_view()}
                            </tbody>
                        </table>
                    }.into_any(),
                    Err(e) => view! { <p class="diag">{e.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>
    }
}
