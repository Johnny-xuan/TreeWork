use crate::project_map_read_model::{
    BranchLookupError, ProjectMapInvalidation, ProjectMapProjection, ProjectMapStore,
};
use crate::project_map_replay::{project_replay, ReplayQueryError, ReplayRequest};
use crate::project_map_watcher::ProjectMapWatcher;
use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, uri::Authority, Method, Request, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive};
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::convert::Infallible;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
#[cfg(unix)]
use std::{
    os::fd::{AsRawFd, FromRawFd},
    os::unix::ffi::OsStrExt,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[derive(Clone)]
struct ServerState {
    root: Arc<PathBuf>,
    output: Arc<OutputRoot>,
    store: Arc<ProjectMapStore>,
    invalidations: broadcast::Sender<ProjectMapInvalidation>,
    once_shutdown: Option<watch::Sender<bool>>,
}

#[derive(Clone, Debug)]
struct OutputRoot {
    path: PathBuf,
    canonical: PathBuf,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

#[derive(Debug)]
enum AssetReadError {
    InvalidPath,
    RootUnavailable,
    NotFound,
    Read(io::Error),
}

impl OutputRoot {
    fn capture(workspace: &Path) -> Result<Self, String> {
        let treework = workspace.join(".TreeWork");
        let path = treework.join("out");
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {}", path.display(), error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "Project Map output root must be a real directory: {}",
                path.display()
            ));
        }
        let canonical_treework = fs::canonicalize(&treework)
            .map_err(|error| format!("cannot resolve {}: {}", treework.display(), error))?;
        let canonical = fs::canonicalize(&path)
            .map_err(|error| format!("cannot resolve {}: {}", path.display(), error))?;
        if canonical.parent() != Some(canonical_treework.as_path()) {
            return Err(format!(
                "Project Map output root escapes .TreeWork: {}",
                path.display()
            ));
        }
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {}", path.display(), error))?;
        Ok(Self {
            path,
            canonical,
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }

    fn verify_current(&self) -> Result<(), AssetReadError> {
        let metadata =
            fs::symlink_metadata(&self.path).map_err(|_| AssetReadError::RootUnavailable)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AssetReadError::RootUnavailable);
        }
        let canonical =
            fs::canonicalize(&self.path).map_err(|_| AssetReadError::RootUnavailable)?;
        if canonical != self.canonical {
            return Err(AssetReadError::RootUnavailable);
        }
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&self.path).map_err(|_| AssetReadError::RootUnavailable)?;
            if metadata.dev() != self.device || metadata.ino() != self.inode {
                return Err(AssetReadError::RootUnavailable);
            }
        }
        Ok(())
    }

    fn read(&self, relative: &str) -> Result<Vec<u8>, AssetReadError> {
        let relative = Path::new(relative);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(AssetReadError::InvalidPath);
        }
        self.verify_current()?;
        let result = self.read_anchored(relative);
        self.verify_current()?;
        result
    }

    #[cfg(unix)]
    fn read_anchored(&self, relative: &Path) -> Result<Vec<u8>, AssetReadError> {
        let mut current = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(&self.path)
            .map_err(|_| AssetReadError::RootUnavailable)?;
        let metadata = current
            .metadata()
            .map_err(|_| AssetReadError::RootUnavailable)?;
        if metadata.dev() != self.device || metadata.ino() != self.inode {
            return Err(AssetReadError::RootUnavailable);
        }

        let components = relative.components().collect::<Vec<_>>();
        if components.is_empty() {
            return Err(AssetReadError::InvalidPath);
        }
        for (index, component) in components.iter().enumerate() {
            let Component::Normal(name) = component else {
                return Err(AssetReadError::InvalidPath);
            };
            let name = CString::new(name.as_bytes()).map_err(|_| AssetReadError::InvalidPath)?;
            let final_component = index + 1 == components.len();
            let flags = libc::O_RDONLY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | if final_component {
                    0
                } else {
                    libc::O_DIRECTORY
                };
            let descriptor = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
            if descriptor < 0 {
                let error = io::Error::last_os_error();
                return Err(if error.raw_os_error() == Some(libc::ELOOP) {
                    AssetReadError::InvalidPath
                } else {
                    AssetReadError::NotFound
                });
            }
            current = unsafe { File::from_raw_fd(descriptor) };
            let metadata = current.metadata().map_err(AssetReadError::Read)?;
            if final_component {
                if !metadata.is_file() {
                    return Err(AssetReadError::InvalidPath);
                }
            } else if !metadata.is_dir() {
                return Err(AssetReadError::InvalidPath);
            }
        }
        let mut body = Vec::new();
        current
            .read_to_end(&mut body)
            .map_err(AssetReadError::Read)?;
        Ok(body)
    }

    #[cfg(not(unix))]
    fn read_anchored(&self, relative: &Path) -> Result<Vec<u8>, AssetReadError> {
        let mut path = self.path.clone();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(AssetReadError::InvalidPath);
            };
            path.push(name);
            let metadata = fs::symlink_metadata(&path).map_err(|_| AssetReadError::NotFound)?;
            if metadata.file_type().is_symlink() {
                return Err(AssetReadError::InvalidPath);
            }
        }
        if !fs::metadata(&path)
            .map_err(|_| AssetReadError::NotFound)?
            .is_file()
        {
            return Err(AssetReadError::InvalidPath);
        }
        fs::read(path).map_err(AssetReadError::Read)
    }
}

impl ServerState {
    fn mark_request(&self) {
        if let Some(shutdown) = &self.once_shutdown {
            let _ = shutdown.send(true);
        }
    }
}

#[derive(Deserialize)]
struct BranchQuery {
    id: String,
}

pub(crate) fn serve(root: &Path, port: u16, once: bool) -> super::AppResult<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            super::AppError(format!("failed to start Project Map runtime: {}", error))
        })?;
    runtime.block_on(serve_async(root.to_path_buf(), port, once))
}

async fn serve_async(root: PathBuf, port: u16, once: bool) -> super::AppResult<()> {
    let output = Arc::new(OutputRoot::capture(&root).map_err(super::AppError)?);
    let store = Arc::new(ProjectMapStore::new(root.clone()));
    let _ = store.refresh();
    let (invalidations, _) = broadcast::channel(64);
    let watcher =
        ProjectMapWatcher::spawn(store.clone(), invalidations.clone()).map_err(super::AppError)?;
    let (once_shutdown, mut once_receiver) = watch::channel(false);
    let state = ServerState {
        root: Arc::new(root),
        output,
        store,
        invalidations,
        once_shutdown: once.then_some(once_shutdown),
    };
    let application = router(state);
    let listener = bind_localhost(port).await.map_err(|error| {
        super::AppError(format!(
            "failed to bind TreeWork graph server on 127.0.0.1:{}: {}",
            port, error
        ))
    })?;
    let address = listener.local_addr()?;
    println!(
        "Serving TreeWork Project Map at http://{}/project-map.html",
        address
    );
    io::stdout().flush()?;

    let shutdown = async move {
        if once {
            while !*once_receiver.borrow() {
                if once_receiver.changed().await.is_err() {
                    break;
                }
            }
        } else {
            let _ = tokio::signal::ctrl_c().await;
        }
    };
    let result = axum::serve(listener, application)
        .with_graceful_shutdown(shutdown)
        .await;
    drop(watcher);
    result.map_err(|error| super::AppError(format!("Project Map server failed: {}", error)))
}

fn router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/project-map.html", get(serve_index))
        .route("/api/project-map", get(project_map))
        .route("/api/project-map/branch", get(branch_detail))
        .route("/api/project-map/replay", get(replay))
        .route("/api/project-map/events", get(project_map_events))
        .route("/api/graph", get(compatibility_graph))
        .route("/graph.json", get(serve_named_asset))
        .route("/app.js", get(serve_named_asset))
        .route("/styles.css", get(serve_named_asset))
        .route("/vendor/{*path}", get(serve_vendor_asset))
        .fallback(fallback)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            enforce_request_boundary,
        ))
        .with_state(state)
}

async fn enforce_request_boundary(
    State(state): State<ServerState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !trusted_request_authority(&request) {
        return error_response(
            StatusCode::FORBIDDEN,
            "Project Map accepts only localhost Host and Origin values",
        );
    }
    let is_event_stream = request.uri().path() == "/api/project-map/events";
    if !is_event_stream {
        state.mark_request();
    }
    if request.method() != Method::GET {
        return error_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
    }
    next.run(request).await
}

fn trusted_request_authority(request: &Request<Body>) -> bool {
    let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Ok(authority) = host.parse::<Authority>() else {
        return false;
    };
    if !trusted_loopback_host(authority.host()) {
        return false;
    }
    let Some(origin) = request.headers().get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    matches!(uri.scheme_str(), Some("http" | "https"))
        && uri.host().is_some_and(trusted_loopback_host)
}

fn trusted_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || matches!(host, "::1" | "[::1]")
}

async fn bind_localhost(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await
}

async fn project_map(State(state): State<ServerState>) -> Response {
    match state.store.projection() {
        Ok(projection) => (StatusCode::OK, Json(projection)).into_response(),
        Err(unavailable) => (StatusCode::SERVICE_UNAVAILABLE, Json(unavailable)).into_response(),
    }
}

async fn branch_detail(
    State(state): State<ServerState>,
    Query(query): Query<BranchQuery>,
) -> Response {
    match state.store.branch_detail(&query.id) {
        Ok(detail) => (StatusCode::OK, Json(detail)).into_response(),
        Err(BranchLookupError::Invalid(message)) => {
            error_response(StatusCode::BAD_REQUEST, message)
        }
        Err(BranchLookupError::Unknown(id)) => {
            error_response(StatusCode::NOT_FOUND, format!("unknown branch `{}`", id))
        }
        Err(BranchLookupError::Unavailable(unavailable)) => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(unavailable)).into_response()
        }
    }
}

async fn replay(State(state): State<ServerState>, Query(query): Query<ReplayRequest>) -> Response {
    replay_with_projector(&state, query, 50, Duration::from_millis(40), project_replay).await
}

async fn replay_with_projector<T, F>(
    state: &ServerState,
    query: ReplayRequest,
    max_convergence_retries: usize,
    retry_delay: Duration,
    mut projector: F,
) -> Response
where
    T: Serialize,
    F: FnMut(&Path, &ProjectMapProjection, ReplayRequest) -> Result<T, ReplayQueryError>,
{
    let mut convergence_attempt = 0;
    loop {
        if let Some(invalidation) = state.store.refresh().invalidation {
            let _ = state.invalidations.send(invalidation);
        }
        let live = match state.store.projection() {
            Ok(projection) => projection,
            Err(unavailable) => {
                return (StatusCode::SERVICE_UNAVAILABLE, Json(unavailable)).into_response()
            }
        };
        match projector(&state.root, &live, query.clone()) {
            Err(ReplayQueryError::Unavailable(message))
                if (message.contains("has not converged with the publication marker")
                    || message.contains("publication lock is active")
                    || message.contains("publication marker changed during Replay read"))
                    && convergence_attempt < max_convergence_retries =>
            {
                convergence_attempt += 1;
                tokio::time::sleep(retry_delay).await;
            }
            Ok(response) => return (StatusCode::OK, Json(response)).into_response(),
            Err(ReplayQueryError::BadRequest(message)) => {
                return error_response(StatusCode::BAD_REQUEST, message)
            }
            Err(ReplayQueryError::NotFound(branch)) => {
                return error_response(
                    StatusCode::NOT_FOUND,
                    format!("unknown branch `{}`", branch),
                )
            }
            Err(ReplayQueryError::Unavailable(message)) => {
                return error_response(StatusCode::SERVICE_UNAVAILABLE, message)
            }
        }
    }
}

async fn project_map_events(
    State(state): State<ServerState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.invalidations.subscribe())
        .take_while(|message| message.is_ok())
        .filter_map(|message| {
            let invalidation = message.ok()?;
            let data = serde_json::to_string(&invalidation).ok()?;
            Some(Ok(Event::default().event("invalidate").data(data)))
        });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn serve_index(State(state): State<ServerState>) -> Response {
    serve_asset(&state.output, "project-map.html")
}

async fn serve_named_asset(State(state): State<ServerState>, request: Request<Body>) -> Response {
    let name = request.uri().path().trim_start_matches('/');
    serve_asset(&state.output, name)
}

async fn serve_vendor_asset(
    State(state): State<ServerState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    serve_asset(&state.output, &format!("vendor/{}", path))
}

async fn compatibility_graph(State(state): State<ServerState>) -> Response {
    match state
        .output
        .read("graph.json")
        .map_err(|error| format!("{:?}", error))
        .and_then(|source| {
            serde_json::from_slice::<serde_json::Value>(&source).map_err(|e| e.to_string())
        }) {
        Ok(graph) => (StatusCode::OK, Json(json!({"ok": true, "graph": graph}))).into_response(),
        Err(error) => error_response(
            StatusCode::NOT_FOUND,
            format!("compatibility graph is unavailable: {}", error),
        ),
    }
}

async fn fallback() -> Response {
    error_response(StatusCode::NOT_FOUND, "not found")
}

fn serve_asset(output: &OutputRoot, relative: &str) -> Response {
    match output.read(relative) {
        Ok(body) => {
            let content_type = content_type(relative);
            (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body).into_response()
        }
        Err(AssetReadError::InvalidPath) => {
            error_response(StatusCode::BAD_REQUEST, "invalid asset path")
        }
        Err(AssetReadError::RootUnavailable) => {
            error_response(StatusCode::NOT_FOUND, "Project Map assets are unavailable")
        }
        Err(AssetReadError::NotFound) => error_response(StatusCode::NOT_FOUND, "asset not found"),
        Err(AssetReadError::Read(error)) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cannot read asset: {}", error),
        ),
    }
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else {
        "application/octet-stream"
    }
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({
            "ok": false,
            "error": message.into(),
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_map_read_model::test_support::TestFixture;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn test_output(root: &Path) -> Arc<OutputRoot> {
        let output = root.join(".TreeWork/out");
        fs::create_dir_all(output.join("vendor")).expect("create test output");
        for (name, body) in [
            ("project-map.html", b"<main>Project Map</main>".as_slice()),
            ("app.js", b"console.log('Project Map')".as_slice()),
            ("styles.css", b"body {}".as_slice()),
            ("graph.json", br#"{"nodes":[],"edges":[]}"#.as_slice()),
        ] {
            fs::write(output.join(name), body).expect("write test asset");
        }
        Arc::new(OutputRoot::capture(root).expect("capture test output"))
    }

    fn unavailable_router() -> Router {
        let root = tempdir().expect("temporary directory").keep();
        let output = test_output(&root);
        let store = Arc::new(ProjectMapStore::new(root.clone()));
        let _ = store.refresh();
        let (invalidations, _) = broadcast::channel(8);
        router(ServerState {
            root: Arc::new(root),
            output,
            store,
            invalidations,
            once_shutdown: None,
        })
    }

    fn accepted_router() -> (
        TestFixture,
        Router,
        broadcast::Sender<ProjectMapInvalidation>,
    ) {
        let fixture = TestFixture::accepted();
        let store = Arc::new(fixture.store());
        let _ = store.refresh();
        let (invalidations, _) = broadcast::channel(8);
        let output = test_output(&fixture.root);
        let application = router(ServerState {
            root: Arc::new(fixture.root.clone()),
            output,
            store,
            invalidations: invalidations.clone(),
            once_shutdown: None,
        });
        (fixture, application, invalidations)
    }

    fn local_request(method: Method, uri: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "127.0.0.1")
            .body(Body::empty())
            .expect("request")
    }

    #[tokio::test]
    async fn returns_structured_503_without_last_good_projection() {
        let response = unavailable_router()
            .oneshot(local_request(Method::GET, "/api/project-map"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("503 body")
            .to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).expect("503 JSON");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["health"]["status"], "unavailable");
    }

    #[tokio::test]
    async fn routes_are_get_only() {
        for method in [
            Method::HEAD,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ] {
            let response = unavailable_router()
                .oneshot(local_request(method, "/api/project-map"))
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }

        let response = unavailable_router()
            .oneshot(local_request(Method::POST, "/unknown"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn request_boundary_rejects_non_local_hosts_and_origins() {
        let missing_host = unavailable_router()
            .oneshot(
                Request::get("/api/project-map")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(missing_host.status(), StatusCode::FORBIDDEN);

        let foreign_host = unavailable_router()
            .oneshot(
                Request::get("/api/project-map")
                    .header(header::HOST, "treework.example")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(foreign_host.status(), StatusCode::FORBIDDEN);

        let foreign_origin = unavailable_router()
            .oneshot(
                Request::get("/api/project-map")
                    .header(header::HOST, "localhost:8791")
                    .header(header::ORIGIN, "https://treework.example")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(foreign_origin.status(), StatusCode::FORBIDDEN);

        let localhost = unavailable_router()
            .oneshot(
                Request::get("/api/project-map")
                    .header(header::HOST, "localhost:8791")
                    .header(header::ORIGIN, "http://localhost:8791")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(localhost.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn listener_binds_only_ipv4_localhost() {
        let listener = bind_localhost(0).await.expect("bind localhost");
        let address = listener.local_addr().expect("local address");
        assert_eq!(address.ip(), std::net::IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn asset_serving_stays_within_disposable_output() {
        let root = tempdir().expect("temporary directory");
        let output = root.path().join(".TreeWork/out");
        fs::create_dir_all(output.join("vendor")).expect("create output");
        fs::write(output.join("app.js"), b"console.log('ok')").expect("write asset");
        let captured = OutputRoot::capture(root.path()).expect("capture output");

        assert_eq!(serve_asset(&captured, "app.js").status(), StatusCode::OK);
        assert_eq!(
            serve_asset(&captured, "../state/project.json").status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            serve_asset(&captured, "vendor/../../events.jsonl").status(),
            StatusCode::BAD_REQUEST
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let outside = root.path().join("outside.js");
            fs::write(&outside, b"secret").expect("write outside file");
            symlink(&outside, output.join("vendor/escape.js")).expect("create asset symlink");
            assert_eq!(
                serve_asset(&captured, "vendor/escape.js").status(),
                StatusCode::BAD_REQUEST
            );

            let parked = root.path().join(".TreeWork/out.original");
            let outside_output = root.path().join("outside-output");
            fs::create_dir(&outside_output).expect("outside output");
            fs::write(outside_output.join("app.js"), b"outside sentinel").expect("outside app");
            fs::rename(&output, &parked).expect("park output");
            symlink(&outside_output, &output).expect("replace output with symlink");
            assert_eq!(
                serve_asset(&captured, "app.js").status(),
                StatusCode::NOT_FOUND
            );
            fs::remove_file(&output).expect("remove output symlink");
            fs::rename(&parked, &output).expect("restore output");
            assert_eq!(serve_asset(&captured, "app.js").status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn sse_route_uses_event_stream_content_type() {
        let response = unavailable_router()
            .oneshot(local_request(Method::GET, "/api/project-map/events"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
    }

    #[tokio::test]
    async fn current_state_and_branch_routes_return_contract_shapes() {
        let (_fixture, application, _) = accepted_router();
        let response = application
            .clone()
            .oneshot(local_request(Method::GET, "/api/project-map"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("projection body")
            .to_bytes();
        let projection: serde_json::Value = serde_json::from_slice(&body).expect("projection JSON");
        assert_eq!(projection["schema_version"], 1);
        assert_eq!(projection["project"]["topology_source"], "accepted");
        assert_eq!(projection["nodes"][2]["id"], "feature");
        assert_eq!(projection["nodes"][2]["readiness"], "ready");

        let response = application
            .oneshot(local_request(
                Method::GET,
                "/api/project-map/branch?id=feature",
            ))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("branch body")
            .to_bytes();
        let detail: serde_json::Value = serde_json::from_slice(&body).expect("branch JSON");
        assert_eq!(detail["branch"]["id"], "feature");
        assert_eq!(detail["task_plan"]["scope"], "Scope body.");
        assert_eq!(
            detail["progress"]["current_reality"],
            "initial feature reality"
        );
    }

    #[tokio::test]
    async fn replay_waits_for_a_short_publication_lock() {
        let (fixture, application, _) = accepted_router();
        let lock = fixture.root.join(".TreeWork.lock");
        fs::create_dir(&lock).expect("publication lock");
        let release_lock = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            fs::remove_dir(lock).expect("release publication lock");
        });

        let started = std::time::Instant::now();
        let response = application
            .oneshot(local_request(Method::GET, "/api/project-map/replay"))
            .await
            .expect("Replay response");

        release_lock.await.expect("lock release task");
        let body = response
            .into_body()
            .collect()
            .await
            .expect("Replay body")
            .to_bytes();
        assert!(started.elapsed() >= Duration::from_millis(80));
        assert!(!String::from_utf8_lossy(&body).contains("publication lock is active"));
    }

    #[tokio::test]
    async fn replay_retries_marker_change_but_not_persistent_or_other_damage() {
        let fixture = TestFixture::accepted();
        let store = Arc::new(fixture.store());
        let _ = store.refresh();
        let (invalidations, _) = broadcast::channel(8);
        let state = ServerState {
            root: Arc::new(fixture.root.clone()),
            output: test_output(&fixture.root),
            store,
            invalidations,
            once_shutdown: None,
        };

        let transient_attempts = Arc::new(AtomicUsize::new(0));
        let transient_counter = transient_attempts.clone();
        let transient = replay_with_projector(
            &state,
            ReplayRequest::default(),
            2,
            Duration::ZERO,
            move |_, _, _| {
                if transient_counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(ReplayQueryError::Unavailable(
                        "publication marker changed during Replay read".to_string(),
                    ))
                } else {
                    Ok(json!({"reconstruction": {"status": "available"}}))
                }
            },
        )
        .await;
        assert_eq!(transient.status(), StatusCode::OK);
        assert_eq!(transient_attempts.load(Ordering::SeqCst), 2);

        let persistent_attempts = Arc::new(AtomicUsize::new(0));
        let persistent_counter = persistent_attempts.clone();
        let persistent = replay_with_projector::<serde_json::Value, _>(
            &state,
            ReplayRequest::default(),
            2,
            Duration::ZERO,
            move |_, _, _| {
                persistent_counter.fetch_add(1, Ordering::SeqCst);
                Err(ReplayQueryError::Unavailable(
                    "publication marker changed during Replay read".to_string(),
                ))
            },
        )
        .await;
        assert_eq!(persistent.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(persistent_attempts.load(Ordering::SeqCst), 3);

        let corrupt_attempts = Arc::new(AtomicUsize::new(0));
        let corrupt_counter = corrupt_attempts.clone();
        let corrupt = replay_with_projector::<serde_json::Value, _>(
            &state,
            ReplayRequest::default(),
            2,
            Duration::ZERO,
            move |_, _, _| {
                corrupt_counter.fetch_add(1, Ordering::SeqCst);
                Err(ReplayQueryError::Unavailable(
                    "checkpoint digest is invalid".to_string(),
                ))
            },
        )
        .await;
        assert_eq!(corrupt.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(corrupt_attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn branch_route_rejects_traversal_and_unknown_ids() {
        let (_fixture, application, _) = accepted_router();
        let traversal = application
            .clone()
            .oneshot(local_request(
                Method::GET,
                "/api/project-map/branch?id=..%2Fescape",
            ))
            .await
            .expect("response");
        assert_eq!(traversal.status(), StatusCode::BAD_REQUEST);

        let unknown = application
            .oneshot(local_request(
                Method::GET,
                "/api/project-map/branch?id=unknown",
            ))
            .await
            .expect("response");
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sse_serializes_one_invalidation_with_batched_categories() {
        let (_fixture, application, invalidations) = accepted_router();
        let response = application
            .oneshot(local_request(Method::GET, "/api/project-map/events"))
            .await
            .expect("response");
        let mut body = response.into_body();
        invalidations
            .send(ProjectMapInvalidation {
                schema_version: 1,
                kind: "project_map.invalidated".to_string(),
                changes: vec!["topology".to_string(), "state".to_string()],
                tree_revision: 2,
                state_event_seq: 3,
                narrative_revision: "sha256:test".to_string(),
            })
            .expect("SSE subscriber");
        let frame = tokio::time::timeout(Duration::from_secs(1), body.frame())
            .await
            .expect("SSE frame timeout")
            .expect("SSE frame")
            .expect("SSE body");
        let data = frame.into_data().expect("SSE data");
        let text = String::from_utf8(data.to_vec()).expect("SSE UTF-8");
        assert!(text.contains("event: invalidate"));
        assert!(text.contains("\"changes\":[\"topology\",\"state\"]"));
        assert!(text.contains("\"kind\":\"project_map.invalidated\""));
    }

    #[tokio::test]
    async fn lagged_sse_subscriber_is_closed_for_authoritative_reconnect() {
        let fixture = TestFixture::accepted();
        let store = Arc::new(fixture.store());
        let _ = store.refresh();
        let (invalidations, _) = broadcast::channel(2);
        let output = test_output(&fixture.root);
        let application = router(ServerState {
            root: Arc::new(fixture.root.clone()),
            output,
            store,
            invalidations: invalidations.clone(),
            once_shutdown: None,
        });
        let response = application
            .oneshot(local_request(Method::GET, "/api/project-map/events"))
            .await
            .expect("response");
        let mut body = response.into_body();
        for sequence in 2..8 {
            invalidations
                .send(ProjectMapInvalidation {
                    schema_version: 1,
                    kind: "project_map.invalidated".to_string(),
                    changes: vec!["state".to_string()],
                    tree_revision: 1,
                    state_event_seq: sequence,
                    narrative_revision: "sha256:test".to_string(),
                })
                .expect("SSE subscriber");
        }
        let frame = tokio::time::timeout(Duration::from_secs(1), body.frame())
            .await
            .expect("lagged stream timeout");
        assert!(
            frame.is_none(),
            "a lagged SSE stream must close so EventSource refetches on reconnect"
        );
    }
}
