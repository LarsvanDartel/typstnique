use std::time::Duration;

use leptos::prelude::*;

use crate::server_fns::{top_scores, LeaderboardPeriod};

/// Milliseconds until the next UTC midnight (start of tomorrow).
fn ms_until_next_utc_midnight() -> f64 {
    let now_ms = js_sys::Date::now();
    let next_midnight = ((now_ms / 86_400_000.0).floor() + 1.0) * 86_400_000.0;
    next_midnight - now_ms
}

/// Milliseconds until midnight UTC on the first of next month.
fn ms_until_next_utc_month() -> f64 {
    let now = js_sys::Date::new_0();
    let year = now.get_utc_full_year();
    let month = now.get_utc_month(); // 0-indexed
    let (next_year, next_month) = if month == 11 {
        (year + 1, 0u32)
    } else {
        (year, month + 1)
    };
    // Date.UTC(year, month) → midnight UTC on the 1st of that month
    js_sys::Date::utc(f64::from(next_year), f64::from(next_month)) - js_sys::Date::now()
}

fn format_countdown(ms: f64) -> String {
    if ms <= 0.0 {
        return "resetting\u{2026}".into();
    }
    let secs = (ms / 1000.0) as u64;
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m {s}s")
    } else {
        format!("{m}m {s}s")
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

    // Countdown to the next reset boundary. Updated every second.
    let countdown = RwSignal::new(String::new());
    Effect::new(move |_| {
        let p = period.get();
        let compute = move || match p {
            LeaderboardPeriod::AllTime => String::new(),
            LeaderboardPeriod::Daily => format_countdown(ms_until_next_utc_midnight()),
            LeaderboardPeriod::Monthly => format_countdown(ms_until_next_utc_month()),
        };
        countdown.set(compute());
        if p == LeaderboardPeriod::AllTime {
            return;
        }
        let handle =
            set_interval_with_handle(move || countdown.set(compute()), Duration::from_secs(1));
        if let Ok(h) = handle {
            on_cleanup(move || h.clear());
        }
    });

    view! {
        <h1>"Leaderboard"</h1>
        <div class="leaderboard-tabs">
            {tab("All time", LeaderboardPeriod::AllTime)}
            {tab("Monthly", LeaderboardPeriod::Monthly)}
            {tab("Daily", LeaderboardPeriod::Daily)}
        </div>
        {move || {
            let c = countdown.get();
            (!c.is_empty()).then(|| view! {
                <p class="reset-hint">"Resets in " {c}</p>
            })
        }}
        <Suspense fallback=move || view! { <p>"Loading\u{2026}"</p> }>
            {move || Suspend::new(async move {
                match scores.await {
                    Ok(list) if list.is_empty() => {
                        view! { <p>"No scores yet \u{2014} be the first!"</p> }.into_any()
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
                    }
                    .into_any(),
                    Err(e) => view! { <p class="diag">{e.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>
    }
}
