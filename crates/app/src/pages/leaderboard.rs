use leptos::prelude::*;

use crate::server_fns::{top_scores, LeaderboardPeriod};

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
