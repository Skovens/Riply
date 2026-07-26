use serde_json::Value;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::RwLock;

use app::JobScheduler;
use infra::{Config, Database};
use web::{create_router, AppState, SseManager};

struct TestContext {
    addr: std::net::SocketAddr,
    state: Arc<AppState>,
    _db_file: NamedTempFile,
    _server: tokio::task::JoinHandle<()>,
}

async fn setup() -> TestContext {
    let db_file = NamedTempFile::new().unwrap();
    let db_path = db_file.path().to_str().unwrap().to_string();
    let db = Database::new(&db_path).await.unwrap();
    db.migrate().await.unwrap();

    let state = Arc::new(AppState {
        db: Arc::new(db),
        config: Arc::new(RwLock::new(Config::default())),
        config_path: None,
        sse: SseManager::new(16),
        backend: None,
        scheduler: JobScheduler::new(),
    });

    let router = create_router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Wait for server to be ready
    let client = reqwest::Client::new();
    for _ in 0..50 {
        if client
            .get(format!("http://{}/health", addr))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    TestContext {
        addr,
        state,
        _db_file: db_file,
        _server: server,
    }
}

async fn get(ctx: &TestContext, uri: &str) -> (u16, Value) {
    let url = format!("http://{}{}", ctx.addr, uri);
    let resp = reqwest::get(&url).await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    let body = serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "raw": text }));
    (status, body)
}

async fn post(ctx: &TestContext, uri: &str, body: Value) -> (u16, Value) {
    let url = format!("http://{}{}", ctx.addr, uri);
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    let body = serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "raw": text }));
    (status, body)
}

async fn put(ctx: &TestContext, uri: &str, body: Value) -> (u16, Value) {
    let url = format!("http://{}{}", ctx.addr, uri);
    let resp = reqwest::Client::new()
        .put(&url)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    let body = serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "raw": text }));
    (status, body)
}

async fn delete(ctx: &TestContext, uri: &str) -> (u16, Value) {
    let url = format!("http://{}{}", ctx.addr, uri);
    let resp = reqwest::Client::new().delete(&url).send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    let body = serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "raw": text }));
    (status, body)
}

async fn seed_file(
    ctx: &TestContext,
    id: &str,
    filename: &str,
    status: riply_core::PipelineStatus,
) {
    let mut file = riply_core::FileRecord::new(
        id.to_string(),
        std::path::PathBuf::from(format!("/test/{}", filename)),
        filename.to_string(),
        format!("hash_{}", id),
    );
    file.update_status(status);
    ctx.state.db.save_file(&file).await.unwrap();
}

// ===== Health =====

#[tokio::test]
async fn test_health_check() {
    let ctx = setup().await;
    let (status, body) = get(&ctx, "/health").await;
    assert_eq!(status, 200);
    assert_eq!(body["raw"], "OK");
}

// ===== Files CRUD =====

#[tokio::test]
async fn test_list_files_empty() {
    let ctx = setup().await;
    let (status, body) = get(&ctx, "/api/files").await;
    assert_eq!(status, 200);
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["items"], serde_json::json!([]));
    assert_eq!(body["data"]["total"], 0);
}

#[tokio::test]
async fn test_list_files_with_data() {
    let ctx = setup().await;
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::Scanned).await;
    seed_file(&ctx, "f2", "b.mp4", riply_core::PipelineStatus::Matched).await;

    let (status, body) = get(&ctx, "/api/files").await;
    assert_eq!(status, 200);
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["total"], 2);
}

#[tokio::test]
async fn test_list_files_filter_by_status() {
    let ctx = setup().await;
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::NeedsReview).await;
    seed_file(&ctx, "f2", "b.mp4", riply_core::PipelineStatus::Scanned).await;

    let (status, body) = get(&ctx, "/api/files?status=Needs%20Review").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["total"], 1);

    let (status, body) = get(&ctx, "/api/files?status=Scanned").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["total"], 1);
}

#[tokio::test]
async fn test_get_file_not_found() {
    let ctx = setup().await;
    let (status, _) = get(&ctx, "/api/files/nonexistent").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn test_get_file_found() {
    let ctx = setup().await;
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::Scanned).await;

    let (status, body) = get(&ctx, "/api/files/f1").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "f1");
    assert_eq!(body["data"]["filename"], "a.mkv");
}

#[tokio::test]
async fn test_update_file() {
    let ctx = setup().await;
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::Scanned).await;

    let body = serde_json::json!({
        "canonical_title": "The Movie",
        "canonical_year": 2024
    });
    let (status, resp) = post(&ctx, "/api/files/f1", body).await;
    assert_eq!(status, 200);
    assert_eq!(resp["data"]["canonical_title"], "The Movie");
    assert_eq!(resp["data"]["canonical_year"], 2024);
}

#[tokio::test]
async fn test_update_file_not_found() {
    let ctx = setup().await;
    let (status, _) = post(
        &ctx,
        "/api/files/nonexistent",
        serde_json::json!({ "canonical_title": "X" }),
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn test_delete_file() {
    let ctx = setup().await;
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::Scanned).await;

    let (status, _) = delete(&ctx, "/api/files/f1").await;
    assert_eq!(status, 200);

    let (status, _) = get(&ctx, "/api/files/f1").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn test_delete_file_not_found() {
    let ctx = setup().await;
    let (status, _) = delete(&ctx, "/api/files/nonexistent").await;
    assert_eq!(status, 404);
}

// ===== Review =====

#[tokio::test]
async fn test_review_queue_empty() {
    let ctx = setup().await;
    let (status, body) = get(&ctx, "/api/review").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"], serde_json::json!([]));
}

#[tokio::test]
async fn test_review_queue_has_files() {
    let ctx = setup().await;
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::NeedsReview).await;
    seed_file(&ctx, "f2", "b.mp4", riply_core::PipelineStatus::Scanned).await;

    let (status, body) = get(&ctx, "/api/review").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "f1");
}

#[tokio::test]
async fn test_accept_file() {
    let ctx = setup().await;
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::NeedsReview).await;

    let (status, body) = post(&ctx, "/api/review/f1/accept", serde_json::json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "Confirmed");
}

#[tokio::test]
async fn test_reject_file() {
    let ctx = setup().await;
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::NeedsReview).await;

    let (status, body) = post(&ctx, "/api/review/f1/reject", serde_json::json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "Failed");
}

#[tokio::test]
async fn test_accept_file_not_found() {
    let ctx = setup().await;
    let (status, _) = post(
        &ctx,
        "/api/review/nonexistent/accept",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn test_reject_file_not_found() {
    let ctx = setup().await;
    let (status, _) = post(
        &ctx,
        "/api/review/nonexistent/reject",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, 404);
}

// ===== Move Status =====

#[tokio::test]
async fn test_move_status_empty() {
    let ctx = setup().await;
    let (status, body) = get(&ctx, "/api/move/status").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["confirmed_count"], 0);
    assert_eq!(body["data"]["moved_count"], 0);
}

#[tokio::test]
async fn test_move_status_with_files() {
    let ctx = setup().await;
    seed_file(&ctx, "c1", "c.mkv", riply_core::PipelineStatus::Confirmed).await;
    seed_file(&ctx, "m1", "m.mkv", riply_core::PipelineStatus::Moved).await;
    seed_file(&ctx, "m2", "m2.mkv", riply_core::PipelineStatus::Moved).await;

    let (status, body) = get(&ctx, "/api/move/status").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["confirmed_count"], 1);
    assert_eq!(body["data"]["moved_count"], 2);
}

// ===== Config =====

#[tokio::test]
async fn test_get_config() {
    let ctx = setup().await;
    let (status, body) = get(&ctx, "/api/config").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["database_path"], "riply.db");
    assert_eq!(body["data"]["movies_directory"], "movies");
    assert_eq!(body["data"]["tv_directory"], "tv");
    assert_eq!(body["data"]["host"], "0.0.0.0");
    assert_eq!(body["data"]["port"], 3000);
}

#[tokio::test]
async fn test_update_config() {
    let ctx = setup().await;
    let body = serde_json::json!({
        "movies_directory": "/data/movies",
        "port": 8080
    });
    let (status, resp) = put(&ctx, "/api/config", body).await;
    assert_eq!(status, 200);
    assert_eq!(resp["data"]["movies_directory"], "/data/movies");
    assert_eq!(resp["data"]["port"], 8080);
}

// ===== Stubs =====

#[tokio::test]
async fn test_list_drives() {
    let ctx = setup().await;
    let (status, body) = get(&ctx, "/api/ripper/drives").await;
    assert_eq!(status, 200);
    assert_eq!(body["data"], serde_json::json!([]));
}

#[tokio::test]
async fn test_start_rip_no_makemkv() {
    let ctx = setup().await;
    let (status, _body) = post(&ctx, "/api/ripper/0/start", serde_json::json!({})).await;
    // Returns 503 when MakeMKV is not installed (mkv_client is None in test)
    assert_eq!(status, 503);
}

#[tokio::test]
async fn test_cancel_rip_stub() {
    let ctx = setup().await;
    let (status, body) = post(&ctx, "/api/ripper/0/cancel", serde_json::json!({})).await;
    assert_eq!(status, 200);
    assert!(body["data"]
        .as_str()
        .unwrap()
        .contains("not yet implemented"));
}

// ===== Pagination =====

#[tokio::test]
async fn test_pagination() {
    let ctx = setup().await;
    for i in 0..5 {
        seed_file(
            &ctx,
            &format!("f{}", i),
            &format!("f{}.mkv", i),
            riply_core::PipelineStatus::Scanned,
        )
        .await;
    }

    let (_, body) = get(&ctx, "/api/files?per_page=2&page=1").await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["total"], 5);
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["per_page"], 2);

    let (_, body) = get(&ctx, "/api/files?per_page=2&page=3").await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
}

// ===== Response format =====

#[tokio::test]
async fn test_api_response_success_format() {
    let ctx = setup().await;
    let (_, body) = get(&ctx, "/api/files").await;
    assert!(body.is_object());
    assert!(body.get("success").is_some());
    assert!(body.get("data").is_some());
}

// ===== Debug routing =====

#[tokio::test]
async fn test_debug_routes() {
    let ctx = setup().await;
    let routes = [
        "/health",
        "/api/files",
        "/api/files/f1",
        "/api/review",
        "/api/review/f1/accept",
        "/api/ripper/0/start",
        "/api/config",
        "/api/move/status",
    ];
    for path in &routes {
        let (status, _) = get(&ctx, path).await;
        eprintln!("GET {} -> {}", path, status);
    }
}

#[tokio::test]
async fn test_debug_seed_and_get() {
    let ctx = setup().await;

    // Seed a file
    seed_file(&ctx, "f1", "a.mkv", riply_core::PipelineStatus::Scanned).await;

    // Verify it's in the DB
    let files = ctx.state.db.get_file_by_id("f1").await.unwrap();
    eprintln!("DB lookup for f1: {:?}", files.is_some());

    // Try the HTTP endpoint
    let (status, body) = get(&ctx, "/api/files/f1").await;
    eprintln!("GET /api/files/f1 -> {} body: {}", status, body);

    // Also try get_all_files
    let all = ctx.state.db.get_all_files().await.unwrap();
    eprintln!("DB has {} files", all.len());

    assert_eq!(status, 200);
}
