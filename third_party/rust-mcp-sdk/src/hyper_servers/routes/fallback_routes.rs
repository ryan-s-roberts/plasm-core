use axum::{
    http::{StatusCode, Uri},
    response::IntoResponse,
    Router,
};

pub fn routes() -> Router<()> {
    Router::new().fallback(not_found)
}

pub async fn not_found(uri: Uri) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        format!("The requested uri does not exist:\r\nuri: {uri}"),
    )
}
