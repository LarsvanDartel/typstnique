//! Route page components.

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::game::GameBoard;
use crate::problems::load_problems;
use crate::server_fns::{get_practice_set, save_practice_set, top_scores, LeaderboardPeriod};

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
        <p>
            "Work through the problems at your own pace, or "
            <A href="/practice/new">"build a custom set"</A>
            "."
        </p>
        <GameBoard problems=problems/>
    }
}

/// One problem in the gallery: title, difficulty, and a progressively-filled
/// goal. Clicking (or pressing Enter/Space) opens the enlarged view by setting
/// `selected` to this card's index.
#[component]
fn ProblemCard(
    index: usize,
    title: String,
    stars: String,
    points: u32,
    svg: RwSignal<String>,
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    let open = move || selected.set(Some(index));
    view! {
        <div
            class="panel problem-card"
            role="button"
            tabindex="0"
            on:click=move |_| open()
            on:keydown=move |ev| {
                if ev.key() == "Enter" || ev.key() == " " {
                    ev.prevent_default();
                    open();
                }
            }
        >
            <div class="problem-title">{title}</div>
            <div class="difficulty">{stars} " · " {points} " pts"</div>
            <div class="target" inner_html=move || svg.get()></div>
        </div>
    }
}

/// Enlarged view of one problem: a bigger render of the goal plus its solution
/// (the Typst source that produced it). Closes on the "×", on a backdrop click,
/// or on Escape, by clearing `selected`.
#[component]
fn ProblemModal(
    title: String,
    stars: String,
    points: u32,
    source: String,
    svg: RwSignal<String>,
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    let close = move || selected.set(None);
    // A backdrop <div> only receives key events while focused, so listen on the
    // window for Escape. The handle is removed when the modal is disposed.
    let handle = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            selected.set(None);
        }
    });
    on_cleanup(move || handle.remove());
    view! {
        <div
            class="modal"
            on:click=move |_| close()
        >
            // Stop propagation so clicks inside the box don't reach the backdrop.
            <div class="modal-box modal-box-wide" on:click=|ev| ev.stop_propagation()>
                <button class="modal-close" title="Close" on:click=move |_| close()>"×"</button>
                <h2>{title}</h2>
                <div class="difficulty">{stars} " · " {points} " pts"</div>
                <div class="target" inner_html=move || svg.get()></div>
                <h3>"Solution"</h3>
                <pre class="answer-src">{source}</pre>
            </div>
        </div>
    }
}

/// Problem card variant for the practice set builder. Clicking toggles the
/// problem in/out of the selection rather than opening a modal.
#[component]
fn BuilderCard(
    index: usize,
    title: String,
    stars: String,
    points: u32,
    svg: RwSignal<String>,
    checked: RwSignal<HashSet<usize>>,
) -> impl IntoView {
    let is_checked = move || checked.with(|s| s.contains(&index));
    let toggle = move |_| {
        checked.update(|s| {
            if !s.remove(&index) {
                s.insert(index);
            }
        });
    };
    view! {
        <div
            class="panel problem-card"
            class:selected=is_checked
            role="checkbox"
            aria-checked=move || is_checked().to_string()
            tabindex="0"
            on:click=toggle
            on:keydown=move |ev| {
                if ev.key() == "Enter" || ev.key() == " " {
                    ev.prevent_default();
                    checked.update(|s| {
                        if !s.remove(&index) {
                            s.insert(index);
                        }
                    });
                }
            }
        >
            <div class="card-check" aria-hidden="true">
                {move || if is_checked() { "\u{2713}" } else { "" }}
            </div>
            <div class="problem-title">{title}</div>
            <div class="difficulty">{stars} " · " {points} " pts"</div>
            <div class="target" inner_html=move || svg.get()></div>
        </div>
    }
}

/// How the gallery is ordered. `Original` preserves the bundled order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Original,
    DifficultyAsc,
    DifficultyDesc,
    TitleAz,
}

impl SortKey {
    fn from_value(v: &str) -> Self {
        match v {
            "diff-asc" => Self::DifficultyAsc,
            "diff-desc" => Self::DifficultyDesc,
            "title" => Self::TitleAz,
            _ => Self::Original,
        }
    }
}

/// Precomputed, display-ready metadata for one problem (so filtering/sorting
/// never re-parses the source on each keystroke). `difficulty_score` walks the
/// Typst AST, so doing it 185× per keystroke would be wasteful.
#[derive(Clone)]
struct ProblemMeta {
    title: String,
    title_lower: String,
    source_lower: String,
    stars: String,
    points: u32,
    difficulty: u32,
}

/// Search / difficulty-filter / sort toolbar for the gallery.
#[component]
fn ProblemControls(
    search: RwSignal<String>,
    min_stars: RwSignal<u32>,
    sort: RwSignal<SortKey>,
    shown: Memo<usize>,
) -> impl IntoView {
    view! {
        <div class="problem-controls">
            <input
                class="problem-search"
                type="search"
                placeholder="Search problems…"
                prop:value=move || search.get()
                on:input=move |ev| search.set(event_target_value(&ev))
            />
            <label>
                "Min difficulty "
                <select on:change=move |ev| {
                    min_stars.set(event_target_value(&ev).parse().unwrap_or(1));
                }>
                    <option value="1">"Any"</option>
                    <option value="2">"★★+"</option>
                    <option value="3">"★★★+"</option>
                    <option value="4">"★★★★+"</option>
                    <option value="5">"★★★★★"</option>
                </select>
            </label>
            <label>
                "Sort "
                <select on:change=move |ev| sort.set(SortKey::from_value(&event_target_value(&ev)))>
                    <option value="original">"Original"</option>
                    <option value="diff-asc">"Easiest first"</option>
                    <option value="diff-desc">"Hardest first"</option>
                    <option value="title">"Title A–Z"</option>
                </select>
            </label>
            <span class="problem-count">{move || format!("{} shown", shown.get())}</span>
        </div>
    }
}

/// Browsable, filterable list of all problems. Targets are rendered
/// progressively on the client — one per animation frame — so the page never
/// blocks compiling every formula at once. Panels (title/difficulty) appear
/// immediately; each goal pops in as it's rendered.
#[allow(clippy::too_many_lines)]
#[component]
pub fn ProblemsPage() -> impl IntoView {
    let problems = StoredValue::new(load_problems());
    let count = problems.with_value(Vec::len);
    // Compute each problem's display metadata once, up front.
    let meta_all = StoredValue::new(problems.with_value(|p| {
        p.iter()
            .map(|prob| {
                let difficulty = prob.difficulty();
                ProblemMeta {
                    title: prob.title.clone(),
                    title_lower: prob.title.to_lowercase(),
                    source_lower: prob.source.to_lowercase(),
                    stars: "★".repeat(usize::try_from(difficulty).unwrap_or(0)),
                    points: prob.points(),
                    difficulty,
                }
            })
            .collect::<Vec<_>>()
    }));
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

    // Index of the problem shown in the enlarged modal (`None` = closed).
    let selected = RwSignal::new(None::<usize>);
    // Filter/sort controls.
    let search = RwSignal::new(String::new());
    let sort = RwSignal::new(SortKey::Original);
    let min_stars = RwSignal::new(1u32);

    // Original indices that pass the filters, in the chosen order.
    let visible = Memo::new(move |_| {
        let q = search.get().trim().to_lowercase();
        let min = min_stars.get();
        let key = sort.get();
        meta_all.with_value(|m| {
            let mut idx: Vec<usize> = (0..m.len())
                .filter(|&i| {
                    m[i].difficulty >= min
                        && (q.is_empty()
                            || m[i].title_lower.contains(&q)
                            || m[i].source_lower.contains(&q))
                })
                .collect();
            match key {
                SortKey::Original => {},
                SortKey::DifficultyAsc => idx.sort_by_key(|&i| m[i].points),
                SortKey::DifficultyDesc => idx.sort_by_key(|&i| std::cmp::Reverse(m[i].points)),
                SortKey::TitleAz => idx.sort_by(|&a, &b| m[a].title_lower.cmp(&m[b].title_lower)),
            }
            idx
        })
    });

    let shown = Memo::new(move |_| visible.get().len());

    view! {
        <h1>"Problems"</h1>
        <ProblemControls search=search min_stars=min_stars sort=sort shown=shown/>
        <div class="problem-grid">
            <For each=move || visible.get() key=|i| *i let:i>
                {
                    let (title, stars, points) = meta_all
                        .with_value(|m| (m[i].title.clone(), m[i].stars.clone(), m[i].points));
                    let svg = svgs.with_value(|s| s[i]);
                    view! {
                        <ProblemCard
                            index=i
                            title=title
                            stars=stars
                            points=points
                            svg=svg
                            selected=selected
                        />
                    }
                }
            </For>
        </div>
        {move || visible.get().is_empty().then(|| view! {
            <p class="hint">"No problems match."</p>
        })}
        {move || selected.get().map(|i| {
            let (title, stars, points) = meta_all
                .with_value(|m| (m[i].title.clone(), m[i].stars.clone(), m[i].points));
            let source = problems.with_value(|p| p[i].source.clone());
            let svg = svgs.with_value(|s| s[i]);
            view! {
                <ProblemModal
                    title=title
                    stars=stars
                    points=points
                    source=source
                    svg=svg
                    selected=selected
                />
            }
        })}
    }
}

/// Problem-picker page for building a shareable custom practice set.
/// Same filter/sort/progressive-render machinery as [`ProblemsPage`], but
/// clicking a card toggles its selection instead of opening a modal.
/// After clicking "Create practice link" the server stores the set and returns
/// a UUID; the page shows `/practice/{uuid}` as a copyable shareable URL.
#[allow(clippy::too_many_lines)]
#[component]
pub fn PracticeBuilderPage() -> impl IntoView {
    let problems = StoredValue::new(load_problems());
    let count = problems.with_value(Vec::len);
    let meta_all = StoredValue::new(problems.with_value(|p| {
        p.iter()
            .map(|prob| {
                let difficulty = prob.difficulty();
                ProblemMeta {
                    title: prob.title.clone(),
                    title_lower: prob.title.to_lowercase(),
                    source_lower: prob.source.to_lowercase(),
                    stars: "★".repeat(usize::try_from(difficulty).unwrap_or(0)),
                    points: prob.points(),
                    difficulty,
                }
            })
            .collect::<Vec<_>>()
    }));
    let svgs = StoredValue::new(
        (0..count)
            .map(|_| RwSignal::new(String::new()))
            .collect::<Vec<_>>(),
    );
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

    let checked: RwSignal<HashSet<usize>> = RwSignal::new(HashSet::new());
    let n_checked = move || checked.with(HashSet::len);

    let search = RwSignal::new(String::new());
    let sort = RwSignal::new(SortKey::Original);
    let min_stars = RwSignal::new(1u32);

    let visible = Memo::new(move |_| {
        let q = search.get().trim().to_lowercase();
        let min = min_stars.get();
        let key = sort.get();
        meta_all.with_value(|m| {
            let mut idx: Vec<usize> = (0..m.len())
                .filter(|&i| {
                    m[i].difficulty >= min
                        && (q.is_empty()
                            || m[i].title_lower.contains(&q)
                            || m[i].source_lower.contains(&q))
                })
                .collect();
            match key {
                SortKey::Original => {},
                SortKey::DifficultyAsc => idx.sort_by_key(|&i| m[i].points),
                SortKey::DifficultyDesc => idx.sort_by_key(|&i| std::cmp::Reverse(m[i].points)),
                SortKey::TitleAz => idx.sort_by(|&a, &b| m[a].title_lower.cmp(&m[b].title_lower)),
            }
            idx
        })
    });
    let shown = Memo::new(move |_| visible.get().len());

    let create = Action::new(|titles: &Vec<String>| {
        let titles = titles.clone();
        async move { save_practice_set(titles).await }
    });

    let result_url = move || {
        create.value().get().and_then(|r| r.ok()).map(|uuid| {
            let origin = window().location().origin().unwrap_or_default();
            format!("{origin}/practice/{uuid}")
        })
    };

    let on_create = move |_| {
        let titles: Vec<String> = checked.with(|s| {
            let mut v: Vec<_> = s.iter().copied().collect();
            v.sort_unstable();
            problems.with_value(|p| v.into_iter().map(|i| p[i].title.clone()).collect())
        });
        create.dispatch(titles);
    };

    view! {
        <h1>"Build a practice set"</h1>
        <p>"Select the problems you want to practice, then create a shareable link."</p>
        <ProblemControls search=search min_stars=min_stars sort=sort shown=shown/>
        <div class="problem-grid">
            <For each=move || visible.get() key=|i| *i let:i>
                {
                    let (title, stars, points) = meta_all
                        .with_value(|m| (m[i].title.clone(), m[i].stars.clone(), m[i].points));
                    let svg = svgs.with_value(|s| s[i]);
                    view! {
                        <BuilderCard
                            index=i
                            title=title
                            stars=stars
                            points=points
                            svg=svg
                            checked=checked
                        />
                    }
                }
            </For>
        </div>
        {move || visible.get().is_empty().then(|| view! {
            <p class="hint">"No problems match."</p>
        })}
        <div class="builder-footer">
            <span>{move || format!("{} selected", n_checked())}</span>
            <button
                on:click=on_create
                disabled=move || n_checked() == 0 || create.pending().get()
            >
                {move || if create.pending().get() { "Saving\u{2026}" } else { "Create practice link" }}
            </button>
            {move || result_url().map(|url| view! {
                <div class="practice-set-result">
                    <input
                        readonly
                        value=url.clone()
                        on:click=move |ev| {
                            let _ = event_target::<web_sys::HtmlInputElement>(&ev).select();
                        }
                    />
                    <a href=url>"Open \u{2192}"</a>
                </div>
            })}
            {move || create.value().get().and_then(|r| r.err()).map(|e| view! {
                <p class="diag">{e.to_string()}</p>
            })}
        </div>
    }
}

/// Loads a previously saved custom practice set by UUID and starts a practice
/// session with just those problems. Problems removed from the bundled set
/// since the link was created are silently omitted.
#[component]
pub fn CustomPracticePage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").map(|s| s.to_owned()).unwrap_or_default());

    let set_titles = Resource::new(id, |id| async move { get_practice_set(id).await });

    view! {
        <Suspense fallback=move || view! { <p>"Loading\u{2026}"</p> }>
            {move || Suspend::new(async move {
                match set_titles.await {
                    Err(e) => view! {
                        <p class="diag">"Practice set not found: " {e.to_string()}</p>
                    }.into_any(),
                    Ok(titles) => {
                        let title_set: HashSet<String> = titles.into_iter().collect();
                        let problems: Vec<_> = load_problems()
                            .into_iter()
                            .filter(|p| title_set.contains(&p.title))
                            .collect();
                        if problems.is_empty() {
                            return view! {
                                <p>"This practice set is empty (all its problems may have been removed)."</p>
                            }.into_any();
                        }
                        let n = problems.len();
                        view! {
                            <h1>"Custom practice"</h1>
                            <p>{format!("{n} problem{}", if n == 1 { "" } else { "s" })}</p>
                            <GameBoard problems=problems/>
                        }.into_any()
                    }
                }
            })}
        </Suspense>
    }
}

/// Global leaderboard, fetched from the server.
#[component]
pub fn LeaderboardPage() -> impl IntoView {
    let period = RwSignal::new(LeaderboardPeriod::AllTime);
    let scores = Resource::new(move || period.get(), |p| async move { top_scores(p).await });

    let tab = move |label: &'static str, p: LeaderboardPeriod| {
        view! {
            <button
                class="ghost"
                class:active=move || period.get() == p
                on:click=move |_| period.set(p)
            >{label}</button>
        }
    };

    view! {
        <h1>"Leaderboard"</h1>
        <div class="leaderboard-tabs">
            {tab("All time", LeaderboardPeriod::AllTime)}
            {tab("Monthly", LeaderboardPeriod::Monthly)}
            {tab("Daily", LeaderboardPeriod::Daily)}
        </div>
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
