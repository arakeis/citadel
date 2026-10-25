use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use citadel_core::{contracts::LibraryBookQuery, library::LibraryService, HttpUrlBuilder};
use citadel_opds::{
    credential_store::OpdsCredentialSecret, OpdsBindTarget, OpdsService, OpdsStartConfig,
};
use citadel_server::{api, CalibreCatalogSource};
use libcalibre::{BookAdd, Library};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn fixture() -> (tempfile::TempDir, OpdsService, Router, String) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../libcalibre/tests/fixtures/empty_library/metadata.db"
        ),
        directory.path().join("metadata.db"),
    )
    .unwrap();
    let database = libcalibre::util::get_db_path(directory.path().to_str().unwrap()).unwrap();
    let mut library = Library::new(database).unwrap();
    let source = directory.path().join("source.txt");
    std::fs::write(&source, "HTTP contract").unwrap();
    let book = library
        .add_book(BookAdd {
            title: "Shared Contract".into(),
            author_names: vec!["Test Author".into()],
            tags: Some(vec!["Reference".into()]),
            series: Some("Contracts".into()),
            series_index: None,
            publisher: None,
            publication_date: None,
            rating: None,
            comments: None,
            identifiers: Default::default(),
            language: None,
            file_paths: vec![source],
        })
        .unwrap();
    let cover = image::RgbImage::from_pixel(600, 900, image::Rgb([40, 80, 120]));
    let mut cover_bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut cover_bytes)
        .encode_image(&cover)
        .unwrap();
    library.set_book_cover(book.id, cover_bytes).unwrap();
    drop(library);
    let source = Arc::new(CalibreCatalogSource::open(directory.path()).unwrap());
    let service = OpdsService::with_ephemeral_credentials(
        source.clone(),
        vec![OpdsCredentialSecret {
            username: "reader".into(),
            password: "secret".into(),
        }],
    )
    .unwrap();
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    service
        .start(OpdsStartConfig {
            port: u32::from(port),
            authentication_enabled: true,
            target: OpdsBindTarget::Addresses {
                addresses: vec!["127.0.0.1".into()],
            },
        })
        .await
        .unwrap();
    let router = api::router(source, &service, None);
    (directory, service, router, book.id.as_i32().to_string())
}

async fn request(
    router: &Router,
    method: &str,
    path: &str,
    body: String,
    authenticated: bool,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if authenticated {
        request = request.header(
            "authorization",
            format!("Basic {}", STANDARD.encode("reader:secret")),
        );
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, headers, serde_json::from_slice(&body).unwrap())
}

fn query(limit: Value) -> Value {
    json!({"text":null,"author_id":null,"series_id":null,"tag_id":null,"publisher_id":null,"hide_read":false,"sort":"TitleAsc","limit":limit,"offset":0})
}

#[tokio::test]
async fn http_reads_match_shared_operations_without_changing_database() {
    let (directory, service, router, id) = fixture().await;
    let database_path = directory.path().join("metadata.db");
    let before = std::fs::read(&database_path).unwrap();
    let database = libcalibre::util::get_db_path(directory.path().to_str().unwrap()).unwrap();
    let mut library = Library::open_read_only(database).unwrap();
    let urls = HttpUrlBuilder::new("");
    let mut core = LibraryService::new(&mut library, &urls);
    let input = query(json!(50));
    let expected = core
        .query_books(serde_json::from_value::<LibraryBookQuery>(input.clone()).unwrap())
        .unwrap();
    let (status, _, actual) =
        request(&router, "POST", "/api/books/query", input.to_string(), true).await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(actual, serde_json::to_value(expected).unwrap());
    let (status, _, actual) = request(
        &router,
        "GET",
        &format!("/api/books/{id}"),
        String::new(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        actual,
        serde_json::to_value(core.get_book(&id).unwrap()).unwrap()
    );
    assert!(!actual
        .to_string()
        .contains(directory.path().to_str().unwrap()));
    assert_eq!(std::fs::read(database_path).unwrap(), before);
    service.stop().await;
}

#[tokio::test]
async fn api_auth_errors_bounds_and_schema_are_consistent() {
    let (_directory, service, router, _) = fixture().await;
    let (status, headers, error) =
        request(&router, "GET", "/api/openapi.json", String::new(), false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(headers.contains_key("www-authenticate"));
    assert_eq!(error["code"], "unauthorized");
    let (status, _, schema) =
        request(&router, "GET", "/api/openapi.json", String::new(), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(schema, serde_json::to_value(api::openapi()).unwrap());
    assert_eq!(
        schema["components"]["securitySchemes"]["reader"]["scheme"],
        "basic"
    );
    for (method, path, body, expected, code) in [
        (
            "GET",
            "/api/books/bad",
            String::new(),
            400,
            "invalidArgument",
        ),
        ("GET", "/api/books/99999", String::new(), 404, "notFound"),
        (
            "POST",
            "/api/books/query",
            "{".into(),
            400,
            "invalidArgument",
        ),
        (
            "POST",
            "/api/books/query",
            query(Value::Null).to_string(),
            400,
            "invalidArgument",
        ),
        (
            "POST",
            "/api/books/query",
            query(json!(201)).to_string(),
            400,
            "invalidArgument",
        ),
        (
            "DELETE",
            "/api/books/1",
            String::new(),
            405,
            "invalidArgument",
        ),
    ] {
        let (status, _, error) = request(&router, method, path, body, true).await;
        assert_eq!(status.as_u16(), expected, "{error}");
        assert_eq!(error["code"], code);
    }
    let (status, _, count) = request(
        &router,
        "POST",
        "/api/books/query",
        query(json!(0)).to_string(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{count}");
    assert_eq!(count["total"], 1);
    assert_eq!(count["items"], json!([]));
    let (status, _, error) = request(
        &router,
        "POST",
        "/api/books/query",
        "x".repeat(16 * 1024 + 1),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error["code"], "invalidArgument");
    service.stop().await;
}

async fn asset_request(
    router: &Router,
    method: &str,
    path: &str,
    range: Option<&str>,
    authenticated: bool,
) -> (StatusCode, axum::http::HeaderMap, axum::body::Bytes) {
    let mut request = Request::builder().method(method).uri(path);
    if authenticated {
        request = request.header(
            "authorization",
            format!("Basic {}", STANDARD.encode("reader:secret")),
        );
    }
    if let Some(range) = range {
        request = request.header("range", range);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, headers, bytes)
}

#[tokio::test]
async fn browser_descriptor_facets_and_thumbnail_reads_are_protected_and_path_free() {
    let (directory, service, router, id) = fixture().await;
    let before = std::fs::read(directory.path().join("metadata.db")).unwrap();
    for path in [
        "/api/library",
        "/api/authors",
        "/api/series",
        "/api/tags",
        "/api/publishers",
        "/api/custom-columns",
        "/api/covers/thumbnails",
    ] {
        let (status, _, _) = request(&router, "GET", path, String::new(), false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
        let (status, _, value) = request(&router, "GET", path, String::new(), true).await;
        assert_eq!(status, StatusCode::OK, "{path}: {value}");
        assert!(!value
            .to_string()
            .contains(directory.path().to_str().unwrap()));
        match path {
            "/api/library" => {
                assert!(value["id"].as_str().is_some_and(|id| !id.is_empty()));
                assert_eq!(value["readOnly"], true);
            }
            "/api/authors" => assert_eq!(value[0]["name"], "Test Author"),
            "/api/series" => assert_eq!(value[0]["name"], "Contracts"),
            "/api/tags" => assert_eq!(value[0]["name"], "Reference"),
            "/api/publishers" => assert_eq!(value, json!([])),
            _ => {}
        }
    }
    let input = json!({"book_ids": [id, "99999"]});
    let (status, _, thumbnails) = request(
        &router,
        "POST",
        "/api/covers/thumbnails",
        input.to_string(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{thumbnails}");
    assert_eq!(thumbnails.as_array().unwrap().len(), 1);
    assert_eq!(thumbnails[0]["width"], 300);
    assert_eq!(thumbnails[0]["height"], 450);
    let url = thumbnails[0]["url"].as_str().unwrap();
    let (status, _, _) = asset_request(&router, "GET", url, None, false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, headers, bytes) = asset_request(&router, "GET", url, None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/jpeg");
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (300, 450));
    let (_, _, cached) = request(
        &router,
        "GET",
        "/api/covers/thumbnails",
        String::new(),
        true,
    )
    .await;
    assert_eq!(cached, thumbnails);
    for input in [
        json!({"book_ids": vec!["1"; 201]}),
        json!({"book_ids": ["../1"]}),
    ] {
        let (status, _, _) = request(
            &router,
            "POST",
            "/api/covers/thumbnails",
            input.to_string(),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _, values) = request(
        &router,
        "GET",
        &format!("/api/books/{id}/custom-values"),
        String::new(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(values, json!([]));
    let (status, _, _) = request(
        &router,
        "GET",
        "/api/books/99999/custom-values",
        String::new(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        std::fs::read(directory.path().join("metadata.db")).unwrap(),
        before
    );
    service.stop().await;
}

#[tokio::test]
async fn returned_browser_asset_urls_support_authenticated_downloads_head_and_range() {
    let (_directory, service, router, id) = fixture().await;
    let (_, _, book) = request(
        &router,
        "GET",
        &format!("/api/books/{id}"),
        String::new(),
        true,
    )
    .await;
    let url = book["file_list"][0]["Remote"]["url"].as_str().unwrap();
    let (status, _, _) = asset_request(&router, "GET", url, None, false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, headers, bytes) = asset_request(&router, "GET", url, None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, "HTTP contract");
    assert!(headers["content-disposition"]
        .to_str()
        .unwrap()
        .contains("attachment"));
    let (status, head, bytes) = asset_request(&router, "HEAD", url, None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(head["content-length"], headers["content-length"]);
    assert!(bytes.is_empty());
    let (status, headers, bytes) =
        asset_request(&router, "GET", url, Some("bytes=0-3"), true).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers["content-range"], "bytes 0-3/13");
    assert_eq!(bytes, "HTTP");
    let (status, headers, _) = asset_request(&router, "GET", url, Some("bytes=99-100"), true).await;
    assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(headers["content-range"], "bytes */13");
    let cover_url = book["cover_image"]["url"].as_str().unwrap();
    let (status, headers, bytes) = asset_request(&router, "GET", cover_url, None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/jpeg");
    assert!(image::load_from_memory(&bytes).is_ok());
    for path in [
        format!("/api/books/{id}/files/%2E%2E/metadata.db"),
        format!("/api/books/{id}/files/epub/metadata.db"),
        "/api/books/99999/cover".into(),
    ] {
        let (status, _, _) = asset_request(&router, "GET", &path, None, true).await;
        assert!(status.is_client_error(), "{path}: {status}");
    }
    service.stop().await;
}

#[tokio::test]
async fn browser_hosting_authenticates_navigation_and_only_falls_back_for_html_routes() {
    let (directory, service, _, _) = fixture().await;
    let web = directory.path().join("web");
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::write(web.join("index.html"), "<html>Citadel frontend</html>").unwrap();
    std::fs::write(web.join("assets/app.js"), "window.citadel = true;").unwrap();
    let source = Arc::new(CalibreCatalogSource::open(directory.path()).unwrap());
    let router = api::router(source, &service, Some(&web));
    for (path, accept_html, authenticated, expected, expected_body) in [
        ("/", true, false, 401, None),
        ("/", true, true, 200, Some("<html>Citadel frontend</html>")),
        (
            "/books/1",
            true,
            true,
            200,
            Some("<html>Citadel frontend</html>"),
        ),
        (
            "/assets/app.js",
            false,
            true,
            200,
            Some("window.citadel = true;"),
        ),
        ("/assets/missing.js", true, true, 404, None),
        ("/api/missing", true, true, 404, None),
        ("/books/1", false, true, 404, None),
        ("/%2e%2e/metadata.db", true, true, 404, None),
    ] {
        let mut request = Request::builder().uri(path);
        if accept_html {
            request = request.header("accept", "text/html");
        }
        if authenticated {
            request = request.header(
                "authorization",
                format!("Basic {}", STANDARD.encode("reader:secret")),
            );
        }
        let response = router
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected, "{path}");
        if !authenticated {
            assert!(response.headers().contains_key("www-authenticate"));
        }
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        if let Some(expected) = expected_body {
            assert_eq!(body, expected, "{path}");
        }
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(directory.path().join("metadata.db"), web.join("outside.db"))
            .unwrap();
        let (status, _, _) = asset_request(&router, "GET", "/outside.db", None, true).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    service.stop().await;
}

#[test]
fn committed_openapi_matches_routes() {
    let committed: Value = serde_json::from_str(include_str!("../../../api/openapi.json")).unwrap();
    assert_eq!(
        committed,
        serde_json::to_value(api::openapi()).unwrap(),
        "Run bun api:generate"
    );
}

#[tokio::test]
async fn missing_cover_does_not_fail_other_thumbnails() {
    let (directory, service, router, valid_id) = fixture().await;
    let database = libcalibre::util::get_db_path(directory.path().to_str().unwrap()).unwrap();
    let mut library = Library::new(database).unwrap();
    let missing = library
        .add_book(BookAdd {
            title: "Missing cover".into(),
            author_names: vec!["Test Author".into()],
            tags: None,
            series: None,
            series_index: None,
            publisher: None,
            publication_date: None,
            rating: None,
            comments: None,
            identifiers: Default::default(),
            language: None,
            file_paths: vec![],
        })
        .unwrap();
    let image = image::RgbImage::from_pixel(10, 10, image::Rgb([40, 80, 120]));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut bytes)
        .encode_image(&image)
        .unwrap();
    library.set_book_cover(missing.id, bytes).unwrap();
    let cover = library.resolve_book_cover(missing.id).unwrap();
    std::fs::remove_file(cover.canonical_path()).unwrap();
    drop(library);
    let (status, _, thumbnails) = request(
        &router,
        "POST",
        "/api/covers/thumbnails",
        json!({"book_ids": [valid_id, missing.id.as_i32().to_string()]}).to_string(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{thumbnails}");
    assert_eq!(thumbnails.as_array().unwrap().len(), 1);
    assert_eq!(thumbnails[0]["book_id"], valid_id);
    service.stop().await;
}
