use crate::http::HttpExtensions;
use axum::routing::{get, put};
use axum::{Extension, Router};
use tower_http::cors::CorsLayer;
mod event;
mod pipeline;
mod project;
pub mod subscription;

pub(crate) fn get_router(ext: HttpExtensions) -> Router {
    Router::new()
        .route("/v1/subscriptions", get(subscription::list_subscriptions))
        .route("/v1/subscriptions/:id", get(subscription::get_subscription))
        .route(
            "/v1/subscriptions/:id",
            put(subscription::update_subscription),
        )
        .route("/v1/pipelines", get(pipeline::list_pipelines))
        .route("/v1/events", get(event::list_events))
        // Projects
        .route(
            "/v1/projects",
            get(project::list_projects).post(project::create_project),
        )
        .route(
            "/v1/projects/:id",
            get(project::get_project)
                .put(project::update_project)
                .delete(project::delete_project),
        )
        // Layers
        .layer(Extension(ext.subman))
        .layer(Extension(ext.pipeline_storage))
        .layer(Extension(ext.projects_controller))
        .layer(CorsLayer::permissive())
}
