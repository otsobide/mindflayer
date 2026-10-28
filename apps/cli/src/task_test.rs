//! Unit tests for `task.rs`, kept beside it rather than inside it.

use super::*;

/// `paths` as the owned words a task holds.
fn owned(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn a_directory_made_a_project_is_linked_with_the_rest_in_the_order_given() {
    let task = Task::minds([
        (String::from("beta"), MindsChange::Link),
        (String::from("gamma"), MindsChange::InitAndLink),
        (String::from("alpha"), MindsChange::Unlink),
        (String::from("delta"), MindsChange::InitAndLink),
    ]);

    assert_eq!(
        task,
        Some(Task::Minds {
            init: owned(&["gamma", "delta"]),
            link: owned(&["beta", "gamma", "delta"]),
            unlink: owned(&["alpha"]),
        })
    );
    assert_eq!(
        task.unwrap().commands(),
        [
            owned(&["mind", "-C", "gamma", "init"]),
            owned(&["mind", "-C", "delta", "init"]),
            owned(&["flayer", "link", "beta", "gamma", "delta"]),
            owned(&["flayer", "unlink", "alpha"]),
        ]
    );
}

#[test]
fn nothing_to_link_or_unlink_is_no_task() {
    assert_eq!(Task::minds([]), None);
    assert_eq!(
        Task::minds([(String::from("alpha"), MindsChange::Unlink)]),
        Some(Task::Minds {
            init: Vec::new(),
            link: Vec::new(),
            unlink: owned(&["alpha"]),
        })
    );
}
