#[cfg(test)]
mod route_tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_matchit_param_routing() {
        let mut router = axum::Router::new()
            .route("/health", axum::routing::get(|| async { "ok" }))
            .route("/api/files/{id}", axum::routing::get(|| async { "found" }));

        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        eprintln!("GET /health -> {}", resp.status());

        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/files/f1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        eprintln!("GET /api/files/f1 -> {}", resp.status());
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        eprintln!("body: {}", String::from_utf8_lossy(&body));
    }

    #[tokio::test]
    async fn test_matchit_param_with_state() {
        use axum::extract::State;
        use std::sync::Arc;

        #[derive(Clone)]
        struct MyState(Arc<tokio::sync::RwLock<String>>);

        async fn handler(
            State(state): State<MyState>,
            axum::extract::Path(id): axum::extract::Path<String>,
        ) -> String {
            let val = state.0.read().await;
            format!("{}:{}", id, *val)
        }

        let state = MyState(Arc::new(tokio::sync::RwLock::new("hello".to_string())));
        let mut router = axum::Router::new()
            .route("/api/files/{id}", axum::routing::get(handler))
            .with_state(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/files/f1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        eprintln!("GET /api/files/f1 (with state) -> {}", resp.status());
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        eprintln!("body: {}", String::from_utf8_lossy(&body));
    }

    #[tokio::test]
    async fn test_matchit_param_axum_serve() {
        use std::sync::Arc;

        let mut router = axum::Router::new()
            .route("/health", axum::routing::get(|| async { "ok" }))
            .route("/api/files/{id}", axum::routing::get(|| async { "found" }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        // Wait for server
        let client = reqwest::Client::new();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let resp = client
            .get(format!("http://{}/health", addr))
            .send()
            .await
            .unwrap();
        eprintln!("GET /health -> {}", resp.status());

        let resp = client
            .get(format!("http://{}/api/files/f1", addr))
            .send()
            .await
            .unwrap();
        eprintln!("GET /api/files/f1 -> {}", resp.status());
        let body = resp.text().await.unwrap();
        eprintln!("body: {}", body);
    }
}
