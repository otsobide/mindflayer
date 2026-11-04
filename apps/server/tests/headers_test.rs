//! What each header a guard reads may say, as values drawn anew each run:
//! the Host the server answers to, the Origin it takes a page's requests
//! from, and the cookie and the Bearer token that carry the launch token.
//! The guard suite holds every route to every guard with a header or two;
//! this one holds each guard to every spelling its mothers make, so a check
//! loosened for one of them, or made to depend on case where nothing does,
//! fails here.

mod common;

use common::mother::{
    names, random, BearerMother, CookieMother, HostMother, OriginMother, TokenMother,
};
use common::*;

/// `request` with `name` set to `value`, in place of whatever it held.
fn with(mut request: Request<Body>, name: header::HeaderName, value: &str) -> Request<Body> {
    request.headers_mut().insert(
        name.clone(),
        value
            .parse()
            .unwrap_or_else(|error| panic!("{name}: {value:?}: {error}")),
    );
    request
}

/// A read a script makes: `GET /api/v1/echo`, by Bearer.
fn read(server: &Listening) -> Request<Body> {
    request(
        server,
        Method::GET,
        "/api/v1/echo?echo=hi",
        Auth::Bearer(server.token()),
        None,
    )
}

/// A change the panel's page makes: `POST /api/v1/tasks`, by the cookie,
/// with everything a browser and the panel send beside it.
fn change(server: &Listening) -> Request<Body> {
    let cookie = format!("{}={}", server.cookie(), server.token());
    request(
        server,
        Method::POST,
        "/api/v1/tasks",
        Auth::Cookie(&cookie),
        Some(json!({ "name": "change" })),
    )
}

/// What a refusal of the Host says, which lists the names on the port.
fn answers_only_as(port: u16) -> String {
    let [first, second, third] = names(port);
    format!("this server answers only as {first}, {second} or {third}")
}

#[tokio::test]
async fn the_server_answers_to_its_three_names_on_its_port_in_any_case() {
    let (server, runs) = launch();
    let port = server.address().port();

    for _ in 0..4 {
        for host in HostMother::answered(port) {
            let reply = deliver(&server, with(read(&server), header::HOST, &host)).await;
            assert_eq!(reply.status, StatusCode::OK, "{host}: {}", reply.text());
            assert_eq!(reply.json(), json!({ "echo": "hi" }), "{host}");

            // A page the server served under the name, sending its change.
            let page = OriginMother::of(&host);
            let sent = with(
                with(change(&server), header::HOST, &host),
                header::ORIGIN,
                &page,
            );
            let reply = deliver(&server, sent).await;
            assert_eq!(
                reply.status,
                StatusCode::OK,
                "{host} from {page}: {}",
                reply.text()
            );
        }
    }
    assert_eq!(runs.lock().unwrap().len(), 2 * 4 * 3);
}

#[tokio::test]
async fn every_other_host_is_refused_before_anything_runs() {
    let (server, runs) = launch();
    let port = server.address().port();

    for (why, host) in HostMother::refused(port) {
        // Its own origin as well, which says the page is the name's and so
        // gets it no further.
        let sent = with(change(&server), header::HOST, &host);
        let sent = with(sent, header::ORIGIN, &format!("http://{host}"));
        let reply = deliver(&server, sent).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{why}: {host:?}");
        assert_eq!(
            reply.json(),
            json!({ "error": answers_only_as(port), "code": "unknown_host" }),
            "{why}: {host:?}"
        );

        let reply = deliver(&server, with(read(&server), header::HOST, &host)).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{why}: {host:?}");
        assert_eq!(reply.json()["code"], "unknown_host", "{why}: {host:?}");

        // Outside the API a person reads it, and the launch link sets no
        // cookie for the name.
        let link = format!("/?token={}", server.token());
        let sent = with(
            request(&server, Method::GET, &link, Auth::Nothing, None),
            header::HOST,
            &host,
        );
        let reply = deliver(&server, sent).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{why}: {host:?}");
        assert_eq!(reply.text(), format!("{}\n", answers_only_as(port)));
        assert_eq!(reply.header("set-cookie"), None, "{why}: {host:?}");
    }
    let runs = runs.lock().unwrap();
    assert!(runs.is_empty(), "{runs:?}");
}

#[tokio::test]
async fn a_page_s_origin_is_taken_only_when_it_is_the_name_the_request_reached() {
    let (server, runs) = launch();
    let port = server.address().port();
    let mut taken = 0;

    for name in names(port) {
        let host = random::cased(&name);
        for sent in [change(&server), read(&server)] {
            let sent = with(sent, header::HOST, &host);
            let reply = deliver(
                &server,
                with(sent, header::ORIGIN, &OriginMother::of(&name)),
            )
            .await;
            assert_eq!(reply.status, StatusCode::OK, "{host}: {}", reply.text());
        }
        taken += 1;

        for (why, origin) in OriginMother::foreign(&name, port) {
            // By Bearer as well as by the cookie: whichever way it holds the
            // token, a request a page sent is that page's.
            for sent in [change(&server), read(&server)] {
                let sent = with(with(sent, header::HOST, &host), header::ORIGIN, &origin);
                let reply = deliver(&server, sent).await;
                assert_eq!(
                    reply.status,
                    StatusCode::FORBIDDEN,
                    "{why}: {origin:?} at {host}"
                );
                assert_eq!(
                    reply.json(),
                    json!({
                        "error": format!(
                            "this server takes requests only from its own pages, at http://{name}"
                        ),
                        "code": "foreign_origin",
                    }),
                    "{why}: {origin:?} at {host}"
                );
            }
        }
    }
    // Each change taken ran, and was told its end.
    assert_eq!(runs.lock().unwrap().len(), 2 * taken);
}

#[tokio::test]
async fn a_host_or_an_origin_sent_twice_is_neither() {
    let (server, runs) = launch();
    let port = server.address().port();

    for name in names(port) {
        let mut sent = with(read(&server), header::HOST, &name);
        sent.headers_mut()
            .append(header::HOST, random::cased(&name).parse().unwrap());
        let reply = deliver(&server, sent).await;
        assert_eq!(reply.json()["code"], "unknown_host", "{name} twice");

        let mut sent = with(change(&server), header::HOST, &name);
        for _ in 0..2 {
            sent.headers_mut()
                .append(header::ORIGIN, OriginMother::of(&name).parse().unwrap());
        }
        let reply = deliver(&server, sent).await;
        assert_eq!(
            reply.json()["code"],
            "foreign_origin",
            "{name}'s page twice"
        );
    }
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_cookie_named_for_the_port_is_taken_wherever_it_stands_among_others() {
    let (server, runs) = launch();
    let pair = format!("{}={}", server.cookie(), server.token());

    for _ in 0..12 {
        let cookies = CookieMother::among(&pair);

        let reply = get(&server, "/api/v1/transcript", Auth::Cookie(&cookies)).await;
        assert_eq!(
            reply.status,
            StatusCode::OK,
            "{cookies:?}: {}",
            reply.text()
        );
        let sent = with(change(&server), header::COOKIE, &cookies);
        let reply = deliver(&server, sent).await;
        assert_eq!(
            reply.status,
            StatusCode::OK,
            "{cookies:?}: {}",
            reply.text()
        );
    }
    assert_eq!(runs.lock().unwrap().len(), 2 * 12);
}

#[tokio::test]
async fn a_cookie_that_is_not_the_token_under_its_name_is_no_token() {
    let (server, runs) = launch();
    let port = server.address().port();
    let (name, token) = (server.cookie(), server.token().to_owned());
    let other = common::mother::PortMother::other(port);

    let mut refused: Vec<(&str, String)> = vec![
        ("its name in capitals", format!("MF_{port}={token}")),
        ("another port's name", format!("mw_{other}={token}")),
        (
            "its port with a leading zero",
            format!("mw_0{port}={token}"),
        ),
        ("a prefix to its name", format!("x{name}={token}")),
        ("a space before the equals sign", format!("{name} ={token}")),
        ("a space after the equals sign", format!("{name}= {token}")),
        ("no name", format!("={token}")),
        ("the token alone", token.clone()),
        (
            "a comma where a semicolon goes",
            format!("{}={}, {name}={token}", random::word(), random::hex(8)),
        ),
    ];
    for (why, not) in TokenMother::not(&token) {
        refused.push((why, format!("{name}={not}")));
    }

    for (why, pair) in refused {
        let cookies = CookieMother::among(&pair);
        for sent in [
            request(
                &server,
                Method::GET,
                "/api/v1/transcript",
                Auth::Cookie(&cookies),
                None,
            ),
            with(change(&server), header::COOKIE, &cookies),
        ] {
            let reply = deliver(&server, sent).await;
            assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{why}: {cookies:?}");
            assert_eq!(reply.json()["code"], "token_required", "{why}: {cookies:?}");
        }
    }
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_bearer_token_is_taken_however_its_scheme_is_cased_and_spaced() {
    let (server, runs) = launch();

    for (why, authorization) in BearerMother::taken(server.token()) {
        let sent = with(read(&server), header::AUTHORIZATION, &authorization);
        let reply = deliver(&server, sent).await;
        assert_eq!(reply.status, StatusCode::OK, "{why}: {}", reply.text());

        // A change by Bearer needs nothing a page sends.
        let sent = request(
            &server,
            Method::POST,
            "/api/v1/tasks",
            Auth::Nothing,
            Some(json!({ "name": "scripted" })),
        );
        let reply = deliver(&server, with(sent, header::AUTHORIZATION, &authorization)).await;
        assert_eq!(reply.status, StatusCode::OK, "{why}: {}", reply.text());
    }
    assert_eq!(
        runs.lock().unwrap().len(),
        2 * BearerMother::taken(server.token()).len()
    );
}

#[tokio::test]
async fn an_authorization_that_does_not_bear_the_token_is_no_token() {
    let (server, runs) = launch();

    for (why, authorization) in BearerMother::refused(server.token()) {
        for path in ["/api/v1/transcript", "/api/v1/echo?echo=hi"] {
            let sent = request(&server, Method::GET, path, Auth::Nothing, None);
            let reply = deliver(&server, with(sent, header::AUTHORIZATION, &authorization)).await;
            assert_eq!(
                reply.status,
                StatusCode::UNAUTHORIZED,
                "{why}: {authorization:?}"
            );
            assert_eq!(reply.header("www-authenticate"), Some("Bearer"));
            assert_eq!(reply.json()["code"], "token_required", "{why}");
        }
        let sent = request(
            &server,
            Method::POST,
            "/api/v1/tasks",
            Auth::Nothing,
            Some(json!({ "name": "scripted" })),
        );
        let reply = deliver(&server, with(sent, header::AUTHORIZATION, &authorization)).await;
        assert_eq!(
            reply.status,
            StatusCode::UNAUTHORIZED,
            "{why}: {authorization:?}"
        );
    }
    assert!(runs.lock().unwrap().is_empty());
}

/// Whoever wrote one header that bears the token holds it, whatever else
/// was written beside it: the others are passed over, not held against it.
#[tokio::test]
async fn of_several_authorizations_one_bearing_the_token_is_taken() {
    let (server, _) = launch();
    let token = server.token().to_owned();

    for _ in 0..6 {
        let mut written: Vec<String> = BearerMother::refused(&token)
            .into_iter()
            .map(|(_, authorization)| authorization)
            .filter(|_| random::chance())
            .collect();
        written.push(random::pick(&BearerMother::taken(&token)).1);
        let written = random::shuffled(written);
        let mut sent = request(
            &server,
            Method::GET,
            "/api/v1/transcript",
            Auth::Nothing,
            None,
        );
        for authorization in &written {
            sent.headers_mut()
                .append(header::AUTHORIZATION, authorization.parse().unwrap());
        }

        let reply = deliver(&server, sent).await;

        assert_eq!(reply.status, StatusCode::OK, "{written:?}");
    }
}

/// A Bearer that is not the token is no credential at all, so the cookie
/// beside it is what the request holds, and is held to what a page sends.
#[tokio::test]
async fn beside_a_bearer_that_is_not_the_token_the_cookie_is_what_counts() {
    let (server, runs) = launch();

    for (why, authorization) in BearerMother::refused(server.token()) {
        let sent = with(change(&server), header::AUTHORIZATION, &authorization);
        let reply = deliver(&server, sent).await;
        assert_eq!(reply.status, StatusCode::OK, "{why}: {}", reply.text());

        let mut sent = with(change(&server), header::AUTHORIZATION, &authorization);
        sent.headers_mut().remove("sec-fetch-site");
        let reply = deliver(&server, sent).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{why}");
        assert_eq!(reply.json()["code"], "same_origin_required", "{why}");
    }
    assert_eq!(
        runs.lock().unwrap().len(),
        2 * BearerMother::refused(server.token()).len()
    );
}

/// The launch link's query, decoded as a browser encodes a form: any of the
/// token's digits percent-encoded, in either case, and among other
/// parameters, is the token, and its cookie holds the token as issued.
#[tokio::test]
async fn the_launch_link_trades_its_token_however_its_query_is_encoded() {
    let (server, _) = launch();
    let token = server.token().to_owned();

    for _ in 0..8 {
        let encoded: String = token
            .chars()
            .map(|c| {
                if random::chance() {
                    random::cased(&format!("%{:02x}", c as u32))
                } else {
                    c.to_string()
                }
            })
            .collect();
        let mut parameters: Vec<String> = (0..random::between(0, 3))
            .map(|_| format!("{}={}", random::word(), random::word()))
            .collect();
        parameters.push(format!("token={encoded}"));
        let link = format!("/?{}", random::shuffled(parameters).join("&"));

        let reply = get(&server, &link, Auth::Nothing).await;

        assert_eq!(
            reply.status,
            StatusCode::SEE_OTHER,
            "{link}: {}",
            reply.text()
        );
        let set = reply.header("set-cookie").expect("a cookie");
        assert_eq!(
            set,
            format!(
                "{}={token}; Path=/; HttpOnly; SameSite=Strict",
                server.cookie()
            ),
            "{link}"
        );
    }
}

#[tokio::test]
async fn a_launch_link_holding_anything_but_the_token_sets_no_cookie() {
    let (server, _) = launch();

    for (why, not) in TokenMother::not(server.token()) {
        // As a link carries it: a `"` encoded, which a query cannot hold
        // bare, and a `%` too, so it reaches the check as it is.
        let carried = not.replace('%', "%25").replace('"', "%22");
        let link = format!("/?token={carried}");

        let reply = get(&server, &link, Auth::Nothing).await;

        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{why}: {link}");
        assert_eq!(reply.header("set-cookie"), None, "{why}: {link}");
        assert!(reply.text().contains("not this server's"), "{why}");
    }
}
