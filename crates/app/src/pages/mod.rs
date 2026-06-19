//! Route page components, split by domain.

mod home;
mod leaderboard;
mod play;
mod practice;
mod problems;

pub use home::HomePage;
pub use leaderboard::LeaderboardPage;
pub use play::PlayPage;
pub use practice::{CustomPracticePage, PracticeBuilderPage, PracticePage};
pub use problems::ProblemsPage;
