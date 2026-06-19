use leptos::prelude::*;

use crate::problems::load_problems;

/// How the gallery is ordered. `Original` preserves the bundled order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortKey {
    Original,
    DifficultyAsc,
    DifficultyDesc,
    TitleAz,
}

impl SortKey {
    pub(crate) fn from_value(v: &str) -> Self {
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
pub(crate) struct ProblemMeta {
    pub(crate) title: String,
    pub(crate) title_lower: String,
    pub(crate) source_lower: String,
    pub(crate) stars: String,
    pub(crate) points: u32,
    pub(crate) difficulty: u32,
}

/// Search / difficulty-filter / sort toolbar for the problems gallery and the
/// practice set builder.
#[component]
pub fn ProblemControls(
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
                placeholder="Search problems\u{2026}"
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
                    <option value="title">"Title A\u{2013}Z"</option>
                </select>
            </label>
            <span class="problem-count">{move || format!("{} shown", shown.get())}</span>
        </div>
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
            <div class="difficulty">{stars} " \u{b7} " {points} " pts"</div>
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
        <div class="modal" on:click=move |_| close()>
            // Stop propagation so clicks inside the box don't reach the backdrop.
            <div class="modal-box modal-box-wide" on:click=|ev| ev.stop_propagation()>
                <button class="modal-close" title="Close" on:click=move |_| close()>"\u{d7}"</button>
                <h2>{title}</h2>
                <div class="difficulty">{stars} " \u{b7} " {points} " pts"</div>
                <div class="target" inner_html=move || svg.get()></div>
                <h3>"Solution"</h3>
                <pre class="answer-src">{source}</pre>
            </div>
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
                    stars: "\u{2605}".repeat(usize::try_from(difficulty).unwrap_or(0)),
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
