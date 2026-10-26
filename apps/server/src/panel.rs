//! The web panel's files, embedded in the binary, and every path outside
//! `/api`.

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{ALLOW, CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::{EmbeddedFile, RustEmbed};

/// The panel as `make web/build` leaves it.
///
/// Spelled from `$CARGO_MANIFEST_DIR`, which `interpolate-folder-path`
/// expands, so the path says what it is relative to: this crate, never the
/// directory cargo was started in. `allow_missing`, so the server builds and
/// its tests run on a machine without Node, where nothing has built the
/// panel; it then serves a page that says so.
///
/// A release build embeds what the folder held when this crate compiled. A
/// debug build reads the folder as each request comes in, but only if it
/// existed when the crate compiled, because rust-embed resolves the folder's
/// real path once, then.
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../web/dist"]
#[allow_missing = true]
struct Dist;

/// Where the panel's files come from: the build embedded above, or any other
/// embedded folder, which is how a test serves a panel of its own.
#[derive(Clone, Copy)]
pub struct Panel {
    get: fn(&str) -> Option<EmbeddedFile>,
}

impl Panel {
    /// The files of an embedded folder.
    pub fn embedded<E: RustEmbed>() -> Self {
        Self { get: E::get }
    }
}

/// The panel built into this binary.
impl Default for Panel {
    fn default() -> Self {
        Self::embedded::<Dist>()
    }
}

/// Where Vite writes what it names by content hash. A file there never
/// changes under its name, so a browser may keep it for good; everything else
/// is asked about again each time, so a new build shows on the next load.
const HASHED: &str = "assets/";

/// Shown in place of the panel when this build has none.
const NOT_BUILT: &str = "<!doctype html>
<html lang=\"en\">
<meta charset=\"utf-8\">
<title>Panel not built</title>
<p>panel not built: run <code>make web/build</code></p>
</html>
";

/// Any path outside `/api`: a file of the panel's, or `index.html` for a
/// route the panel's own router draws in the browser.
///
/// No token is asked for. These files are the same for everybody and hold
/// nothing that is not in the repository, and a browser that has not been
/// through the launch link yet still needs the page that tells it so.
pub(crate) async fn serve(State(panel): State<Panel>, method: Method, uri: Uri) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(ALLOW, "GET, HEAD")],
            "The panel only answers GET.\n",
        )
            .into_response();
    }
    // Undecoded: Vite names everything it writes in plain ASCII.
    let path = uri.path().trim_start_matches('/');
    // Climbing out names nothing here, whatever it would name on disk.
    if path.split('/').any(|segment| segment == "..") {
        return missing();
    }
    if let Some(file) = (!path.is_empty()).then(|| (panel.get)(path)).flatten() {
        return file_response(path, file);
    }
    // A hashed file that is not there is one an old page asks for after a
    // rebuild. The page in its place would reach the browser as a script it
    // cannot run, which is harder to read than a 404.
    if path.starts_with(HASHED) {
        return missing();
    }
    match (panel.get)("index.html") {
        Some(file) => file_response("index.html", file),
        None => not_built(),
    }
}

fn file_response(path: &str, file: EmbeddedFile) -> Response {
    let cache = if path.starts_with(HASHED) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (CONTENT_TYPE, file.metadata.mimetype().to_owned()),
            (CACHE_CONTROL, cache.to_owned()),
        ],
        Body::from(file.data),
    )
        .into_response()
}

fn missing() -> Response {
    (StatusCode::NOT_FOUND, "Not found.\n").into_response()
}

/// 503 rather than 404: the server is up and its API answers, and it is the
/// panel that this build is without.
fn not_built() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [
            (CONTENT_TYPE, "text/html; charset=utf-8"),
            (CACHE_CONTROL, "no-cache"),
        ],
        NOT_BUILT,
    )
        .into_response()
}
