use leptos::prelude::*;

use crate::game::GameBoard;

/// Timed 3-minute game (server-scored, feeds the leaderboard).
#[component]
pub fn PlayPage() -> impl IntoView {
    view! {
        <h1>"Typstnique"</h1>
        <p>"Recreate as many formulas as you can in three minutes."</p>
        <GameBoard timed=true server_scored=true/>
    }
}
