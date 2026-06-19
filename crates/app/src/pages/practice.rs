use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use super::problems::{ProblemControls, ProblemMeta, SortKey};
use crate::game::GameBoard;
use crate::problems::load_problems;
use crate::server_fns::{get_practice_set, save_practice_set};

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
            <div class="difficulty">{stars} " \u{b7} " {points} " pts"</div>
            <div class="target" inner_html=move || svg.get()></div>
        </div>
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
                    stars: "\u{2605}".repeat(usize::try_from(difficulty).unwrap_or(0)),
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

    let all_visible_selected = move || {
        let v = visible.get();
        !v.is_empty() && v.iter().all(|&i| checked.with(|s| s.contains(&i)))
    };
    let toggle_all = move |_| {
        let v = visible.get();
        checked.update(|s| {
            if v.iter().all(|&i| s.contains(&i)) {
                for &i in &v {
                    s.remove(&i);
                }
            } else {
                for &i in &v {
                    s.insert(i);
                }
            }
        });
    };

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
            <button
                class="ghost"
                on:click=toggle_all
                disabled=move || visible.get().is_empty()
            >
                {move || if all_visible_selected() { "Deselect all" } else { "Select all" }}
            </button>
            <span>{move || format!("{} selected", n_checked())}</span>
            <button
                on:click=on_create
                disabled=move || n_checked() == 0 || create.pending().get()
            >
                {move || if create.pending().get() {
                    "Saving\u{2026}"
                } else {
                    "Create practice link"
                }}
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
                    }
                    .into_any(),
                    Ok(titles) => {
                        let title_set: HashSet<String> = titles.into_iter().collect();
                        let problems: Vec<_> = load_problems()
                            .into_iter()
                            .filter(|p| title_set.contains(&p.title))
                            .collect();
                        if problems.is_empty() {
                            return view! {
                                <p>
                                    "This practice set is empty (all its problems may have been removed)."
                                </p>
                            }
                            .into_any();
                        }
                        let n = problems.len();
                        view! {
                            <h1>"Custom practice"</h1>
                            <p>{format!("{n} problem{}", if n == 1 { "" } else { "s" })}</p>
                            <GameBoard problems=problems/>
                        }
                        .into_any()
                    }
                }
            })}
        </Suspense>
    }
}
