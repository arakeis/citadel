use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::Request,
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

pub(crate) fn validate(directory: &Path) -> Result<(), String> {
    if !directory.is_absolute() {
        return Err("api.webDirectory must be an absolute path".into());
    }
    let root = directory
        .canonicalize()
        .map_err(|_| "api.webDirectory must be an existing frontend build directory")?;
    let index = root
        .join("index.html")
        .canonicalize()
        .map_err(|_| "api.webDirectory must contain index.html")?;
    if !index.starts_with(&root) || !index.is_file() {
        return Err(
            "api.webDirectory index.html must be a file inside the frontend build directory".into(),
        );
    }
    Ok(())
}

pub(super) fn router(directory: &Path) -> Router {
    let directory = Arc::new(directory.to_path_buf());
    Router::new().fallback(move |request: Request| {
        let directory = directory.clone();
        async move { serve(directory, request).await }
    })
}

async fn serve(directory: Arc<PathBuf>, request: Request) -> Response {
    let missing = || StatusCode::NOT_FOUND.into_response();
    let path = match urlencoding::decode(request.uri().path()) {
        Ok(path) => path,
        Err(_) => return missing(),
    };
    let relative = path.trim_start_matches('/');
    if relative == "api"
        || relative.starts_with("api/")
        || relative == "opds"
        || relative.starts_with("opds/")
    {
        return missing();
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if relative.contains(['\\', '\0'])
        || Path::new(relative)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return missing();
    }
    let root = match tokio::fs::canonicalize(directory.as_ref()).await {
        Ok(root) => root,
        Err(_) => return missing(),
    };
    let target = root.join(relative);
    if let Ok(canonical) = tokio::fs::canonicalize(&target).await {
        if !canonical.starts_with(&root) {
            return missing();
        }
        if canonical.is_file() {
            return match ServeDir::new(&root)
                .append_index_html_on_directories(false)
                .oneshot(request)
                .await
            {
                Ok(response) => response.into_response(),
                Err(error) => match error {},
            };
        }
    }
    let navigation = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().starts_with("text/html"))
        });
    if !navigation
        || relative == "assets"
        || relative.starts_with("assets/")
        || Path::new(relative).extension().is_some()
    {
        return missing();
    }
    let index = match tokio::fs::canonicalize(root.join("index.html")).await {
        Ok(index) if index.starts_with(&root) => index,
        _ => return missing(),
    };
    match ServeFile::new(index).oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(error) => match error {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_build_requires_an_absolute_directory_with_an_index() {
        assert!(validate(Path::new("dist")).is_err());
        let directory = tempfile::tempdir().unwrap();
        assert!(validate(directory.path()).is_err());
        std::fs::write(directory.path().join("index.html"), "frontend").unwrap();
        assert!(validate(directory.path()).is_ok());
    }
}
