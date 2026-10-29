use super::*;

const LINK: &str = "http://127.0.0.1:4000/?token=abc";

fn options(no_open: bool, dev: bool) -> UiOptions {
    UiOptions {
        port: None,
        no_open,
        dev,
    }
}

/// What `announce` printed on each stream, and the link it opened, if any;
/// opened by a stand-in, so no test starts a browser.
fn announced(options: &UiOptions, opener: io::Result<()>) -> (String, String, Option<String>) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut opened = None;
    announce(
        LINK,
        SocketAddr::from(([127, 0, 0, 1], 4000)),
        Path::new("/work"),
        options,
        &mut out,
        &mut err,
        |url| {
            opened = Some(url.to_owned());
            opener
        },
    )
    .unwrap();
    (
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
        opened,
    )
}

#[test]
fn the_link_is_printed_alone_on_stdout_and_opened() {
    let (out, err, opened) = announced(&options(false, false), Ok(()));

    assert_eq!(out, format!("{LINK}\n"));
    assert_eq!(
        err,
        "note: serving the workspace at /work on 127.0.0.1:4000 until Ctrl-C\n"
    );
    assert_eq!(opened.as_deref(), Some(LINK));
}

#[test]
fn with_no_open_the_link_is_printed_and_no_browser_asked_for() {
    let (out, _, opened) = announced(&options(true, false), Ok(()));

    assert_eq!(out, format!("{LINK}\n"));
    assert_eq!(opened, None);
}

/// As on a machine with no display: the link is there to open by hand.
#[test]
fn a_browser_that_cannot_be_opened_is_said_and_nothing_fails() {
    let (out, err, _) = announced(
        &options(false, false),
        Err(io::Error::new(io::ErrorKind::NotFound, "no display")),
    );

    assert_eq!(out, format!("{LINK}\n"));
    assert_eq!(
        err,
        "note: serving the workspace at /work on 127.0.0.1:4000 until Ctrl-C\n\
         warning: no browser could be opened (no display); open the link above in one\n"
    );
}

#[test]
fn through_the_dev_server_the_note_says_where() {
    let (_, err, _) = announced(&options(true, true), Ok(()));

    assert_eq!(
        err,
        "note: serving the workspace at /work on 127.0.0.1:4000, through the panel's dev \
         server at http://127.0.0.1:5173 (`make web/dev`), until Ctrl-C\n"
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
