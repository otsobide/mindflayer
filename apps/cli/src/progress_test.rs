use super::*;

/// Bytes written somewhere a test can read them back from.
#[derive(Clone, Default)]
struct Written(Arc<Mutex<Vec<u8>>>);

impl Write for Written {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Written {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

#[test]
fn each_step_reads_as_what_is_counted_and_how_far_it_has_got() {
    let cases = [
        (Progress::Connecting, "connecting"),
        (
            Progress::Receiving {
                done: 120,
                total: Some(340),
            },
            "receiving objects 120/340",
        ),
        (
            Progress::Resolving {
                done: 7,
                total: None,
            },
            "resolving deltas 7",
        ),
        (
            Progress::CheckingOut {
                done: 0,
                total: None,
            },
            "checking out files",
        ),
        (
            Progress::Harvesting { done: 1, total: 3 },
            "reading folders 1/3",
        ),
        (
            Progress::Placing { done: 0, total: 2 },
            "placing on the shelf 0/2",
        ),
    ];
    for (progress, words) in cases {
        assert_eq!(said(progress), words);
    }
}

#[test]
fn the_line_is_rewritten_in_place_and_wiped_at_the_end() {
    let written = Written::default();
    let line = ProgressLine::to(written.clone());
    let watch = line.watch();

    watch.report(Progress::Receiving {
        done: 12,
        total: Some(340),
    });
    watch.report(Progress::Harvesting { done: 1, total: 3 });
    line.clear();

    // The shorter line covers what is left of the longer one with spaces,
    // and the wipe covers the last.
    assert_eq!(
        written.text(),
        "\rreceiving objects 12/340\rreading folders 1/3     \r                   \r"
    );
}

#[test]
fn a_line_that_drew_nothing_wipes_nothing() {
    let written = Written::default();
    let line = ProgressLine::to(written.clone());

    line.clear();

    assert_eq!(written.text(), "");
}

#[test]
fn a_line_that_is_not_drawn_hands_out_a_watch_that_reports_to_nobody() {
    let line = ProgressLine { drawn: None };
    let watch = line.watch();

    watch.report(Progress::Connecting);
    line.clear();

    // Still a watch that can be cancelled, as every one is.
    watch.cancel();
    assert!(watch.is_cancelled());
}

/// Each step as the panel's server carries it: a word, the words the
/// terminal shows, its counts, and whether a cancel still stops it, which
/// only placing on the shelf does not.
#[cfg(feature = "ui")]
#[test]
fn each_step_is_carried_to_the_panel_with_a_word_its_words_and_its_counts() {
    let carried = |progress| {
        let carried = carried(progress);
        (
            carried.step,
            carried.message,
            carried.done,
            carried.total,
            carried.cancellable,
        )
    };

    assert_eq!(
        carried(Progress::Connecting),
        ("connecting".into(), "connecting".into(), 0, None, true)
    );
    assert_eq!(
        carried(Progress::Receiving {
            done: 120,
            total: Some(340),
        }),
        (
            "receiving".into(),
            "receiving objects 120/340".into(),
            120,
            Some(340),
            true
        )
    );
    assert_eq!(
        carried(Progress::Resolving {
            done: 7,
            total: None,
        }),
        (
            "resolving".into(),
            "resolving deltas 7".into(),
            7,
            None,
            true
        )
    );
    assert_eq!(
        carried(Progress::CheckingOut {
            done: 2,
            total: Some(9),
        }),
        (
            "checking_out".into(),
            "checking out files 2/9".into(),
            2,
            Some(9),
            true
        )
    );
    assert_eq!(
        carried(Progress::Harvesting { done: 1, total: 3 }),
        (
            "harvesting".into(),
            "reading folders 1/3".into(),
            1,
            Some(3),
            true
        )
    );
    assert_eq!(
        carried(Progress::Placing { done: 0, total: 2 }),
        (
            "placing".into(),
            "placing on the shelf 0/2".into(),
            0,
            Some(2),
            false
        )
    );
}
