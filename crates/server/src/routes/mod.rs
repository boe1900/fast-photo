pub mod activity_routes;
pub mod ai_routes;
pub mod album_routes;
pub mod auth_routes;
pub mod face_routes;
pub mod library_routes;
pub mod photo_routes;
pub mod settings_routes;
pub mod tag_routes;

use crate::state::AppState;
use axum::Router;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth_routes::routes())
        .nest("/libraries", library_routes::routes())
        .nest("/photos", photo_routes::routes())
        .nest("/ai", ai_routes::routes())
        .nest("/albums", album_routes::routes())
        .nest("/settings", settings_routes::routes())
        .nest("/share", album_routes::share_routes())
        .merge(face_routes::routes())
        .merge(tag_routes::routes())
        .merge(activity_routes::routes())
}
