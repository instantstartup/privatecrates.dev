//! Publishes that arrive at once: different crates go ahead in parallel; one crate's versions take turns.

mod common;

use std::time::{Duration, Instant};

use common::{Crate, Harness};

const UPLOAD: Duration = Duration::from_millis(800);

#[tokio::test]
async fn different_crates_upload_in_parallel() {
    let h = Harness::start().await;
    h.fake.slow_down("/uploads/", UPLOAD);
    let repo = h.repo("engine");
    let crates: Vec<Crate> = (0..4)
        .map(|i| Crate::new(&format!("engine_part{i}"), "0.1.0", "acme/engine"))
        .collect();
    let started = Instant::now();
    let publish = |i: usize| h.publish_from_ci("acme/engine", repo, &crates[i]);
    tokio::join!(publish(0), publish(1), publish(2), publish(3));
    // Each publish uploads the crate and its provenance, 1.6 s each; one at a time, four would take 6.4 s.
    let took = started.elapsed();
    assert!(took < 4 * 2 * UPLOAD, "{took:?}");
    for krate in &crates {
        let index = h
            .fake
            .file(h.org.storage_repo, &format!("index/en/gi/{}", krate.name));
        assert!(index.is_some(), "{}", krate.name);
    }
}

#[tokio::test]
async fn one_crates_versions_take_turns_and_all_land() {
    let h = Harness::start().await;
    let repo = h.repo("engine");
    // The first publish creates the owners file; the rest follow it.
    h.publish_from_ci(
        "acme/engine",
        repo,
        &Crate::new("engine", "0.1.0", "acme/engine"),
    )
    .await;
    let versions: Vec<Crate> = (2..6)
        .map(|minor| Crate::new("engine", &format!("0.{minor}.0"), "acme/engine"))
        .collect();
    let publish = |i: usize| h.publish_from_ci("acme/engine", repo, &versions[i]);
    tokio::join!(publish(0), publish(1), publish(2), publish(3));
    let index = h
        .fake
        .file(h.org.storage_repo, "index/en/gi/engine")
        .unwrap();
    assert_eq!(index.lines().count(), 5, "{index}");
}
