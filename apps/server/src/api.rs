//! The JSON endpoints under `/api`.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::transcript::Transcript;
use crate::writer::Writer;
use crate::{Backend, Refusal};

/// What every handler under `/api` shares.
pub(crate) struct Api<B: Backend> {
    pub(crate) backend: Arc<B>,
    pub(crate) writer: Writer<B::Task>,
    pub(crate) transcript: Transcript,
}

/// `POST /api/query`: a read, answered by the backend.
///
/// On the blocking pool, because a query reads the disk and the runtime's
/// own threads are for moving bytes: one slow read there would stall every
/// other request waiting on that thread.
pub(crate) async fn query<B: Backend>(
    State(api): State<Arc<Api<B>>>,
    body: Result<Json<B::Query>, JsonRejection>,
) -> Response {
    let query = match body {
        Ok(Json(query)) => query,
        Err(rejection) => return refused(rejection),
    };
    let backend = Arc::clone(&api.backend);
    match tokio::task::spawn_blocking(move || backend.query(query)).await {
        Ok(Ok(answer)) => Json(answer).into_response(),
        Ok(Err(refusal)) => refused_by(refusal),
        Err(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the backend failed while answering the query",
        ),
    }
}

/// `POST /api/commands/preview`: the command lines a task is, and nothing
/// run. On the blocking pool for the same reason as a query: working out a
/// line can mean reading the disk.
pub(crate) async fn preview<B: Backend>(
    State(api): State<Arc<Api<B>>>,
    body: Result<Json<B::Task>, JsonRejection>,
) -> Response {
    let task = match body {
        Ok(Json(task)) => task,
        Err(rejection) => return refused(rejection),
    };
    let backend = Arc::clone(&api.backend);
    match tokio::task::spawn_blocking(move || backend.preview(&task)).await {
        Ok(Ok(lines)) => Json(json!({ "lines": lines })).into_response(),
        Ok(Err(refusal)) => refused_by(refusal),
        Err(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the backend failed while previewing the task",
        ),
    }
}

/// `POST /api/commands`: run a task, once every task that arrived before it
/// has run, and say what its lines were and what came of them; or, when the
/// backend refused it, why nothing of it ran.
pub(crate) async fn commands<B: Backend>(
    State(api): State<Arc<Api<B>>>,
    body: Result<Json<B::Task>, JsonRejection>,
) -> Response {
    let task = match body {
        Ok(Json(task)) => task,
        Err(rejection) => return refused(rejection),
    };
    match api.writer.run(task).await {
        Ok(Ok(entry)) => Json(entry).into_response(),
        Ok(Err(refusal)) => refused_by(refusal),
        Err(message) => error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

/// `GET /api/transcript`: every task the session ran, oldest first.
pub(crate) async fn transcript<B: Backend>(State(api): State<Arc<Api<B>>>) -> Response {
    Json(api.transcript.entries()).into_response()
}

/// Whether a path is the API's: `/api` and everything under it, where every
/// answer is JSON, refusals included.
pub(crate) fn is_api(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

/// Any other path under `/api`: a 404 a script can read, rather than the
/// panel's page, which is what a path outside `/api` that names no file gets.
pub(crate) async fn missing(method: Method, uri: Uri) -> Response {
    error(
        StatusCode::NOT_FOUND,
        &format!("no such endpoint: {method} {}", uri.path()),
    )
}

/// A known path asked for with a method it does not take.
pub(crate) async fn not_allowed(method: Method, uri: Uri) -> Response {
    error(
        StatusCode::METHOD_NOT_ALLOWED,
        &format!("{} does not take {method}", uri.path()),
    )
}

/// A body that is not the JSON a handler expects, in the same shape as every
/// other error here rather than axum's plain text.
pub(crate) fn refused(rejection: JsonRejection) -> Response {
    error(rejection.status(), &rejection.body_text())
}

/// What the backend refused, as `{"error": "...", "code": "..."}`: a 422,
/// as a body it could not read is, since in both the request arrived whole
/// and is not one to act on.
pub(crate) fn refused_by(refusal: Refusal) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "error": refusal.message, "code": refusal.code })),
    )
        .into_response()
}

/// An error, as `{"error": "..."}`.
pub(crate) fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}
