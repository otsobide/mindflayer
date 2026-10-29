use super::*;

const LINK: &str = "http://127.0.0.1:4000/?token=abc";

fn options(dev: bool) -> UiOptions {
    UiOptions {
        port: None,
        no_open: false,
        dev,
    }
}

/// What `announce` printed on each stream.
fn announced(options: &UiOptions) -> (String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    announce(
        LINK,
        SocketAddr::from(([127, 0, 0, 1], 4000)),
        Path::new("/work"),
        options,
        &mut out,
        &mut err,
    )
    .unwrap();
    (
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

/// The link `open` asked for, if any, and what it said on stderr; asked of
/// a stand-in answering `answer`, so no test starts a browser.
fn opened(no_open: bool, answer: io::Result<()>) -> (Option<String>, String) {
    let mut err = Vec::new();
    let mut asked = None;
    open(LINK, no_open, &mut err, |url| {
        asked = Some(url.to_owned());
        answer
    });
    (asked, String::from_utf8(err).unwrap())
}

#[test]
fn the_link_is_printed_alone_on_stdout_and_where_it_serves_on_stderr() {
    let (out, err) = announced(&options(false));

    assert_eq!(out, format!("{LINK}\n"));
    assert_eq!(
        err,
        "note: serving the workspace at /work on 127.0.0.1:4000 until Ctrl-C\n"
    );
}

#[test]
fn through_the_dev_server_the_note_says_where() {
    let (_, err) = announced(&options(true));

    assert_eq!(
        err,
        "note: serving the workspace at /work on 127.0.0.1:4000, through the panel's dev \
         server at http://127.0.0.1:5173 (`make web/dev`), until Ctrl-C\n"
    );
}

#[test]
fn the_link_is_opened_unless_told_not_to() {
    assert_eq!(
        opened(false, Ok(())),
        (Some(LINK.to_owned()), String::new())
    );
    assert_eq!(opened(true, Ok(())), (None, String::new()));
}

/// As on a machine with no display: the link is there to open by hand.
#[test]
fn a_browser_that_cannot_be_opened_is_said_and_nothing_fails() {
    let (_, err) = opened(
        false,
        Err(io::Error::new(io::ErrorKind::NotFound, "no display")),
    );

    assert_eq!(
        err,
        "warning: no browser could be opened (no display); open the link above in one\n"
    );
}

/// The dev server's proxy looks on 7878, so `--dev` alone is enough for
/// `make web/dev`; a port named, 0 included, is the port.
#[test]
fn the_port_is_the_one_named_or_the_dev_servers_proxys_or_any() {
    let named = |port, dev| {
        super::port(&UiOptions {
            port,
            no_open: true,
            dev,
        })
    };

    assert_eq!(named(None, false), 0);
    assert_eq!(named(None, true), 7878);
    assert_eq!(named(Some(9000), false), 9000);
    assert_eq!(named(Some(9000), true), 9000);
    assert_eq!(named(Some(0), true), 0);
}
