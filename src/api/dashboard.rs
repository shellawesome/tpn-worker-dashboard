use axum::response::{Html, Redirect};

/// GET / — Redirect to /dashboard.
pub async fn redirect_to_dashboard() -> Redirect {
    Redirect::permanent("/dashboard")
}

/// GET /dashboard — Embedded HTML dashboard page.
pub async fn dashboard_page() -> Html<&'static str> {
    Html(include_str!("../dashboard.html"))
}
