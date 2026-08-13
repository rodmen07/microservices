#[path = "lib/app_state.rs"]
pub mod app_state;
#[path = "lib/auth.rs"]
pub mod auth;
#[path = "lib/handlers/mod.rs"]
pub mod handlers;
#[path = "lib/models.rs"]
pub mod models;
#[path = "lib/peer_total.rs"]
pub mod peer_total;
#[path = "lib/router.rs"]
pub mod router;

pub use app_state::AppState;
pub use router::build_router;
