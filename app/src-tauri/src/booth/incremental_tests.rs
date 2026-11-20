
use super::*;
fn row(id: usize) -> Row {
    Row {
        source_record_id: format!("synthetic:{id}"),
        product_id: Some(id.to_string()),
        manifest_complete: true,
        ..Row::default()
    }
}
fn base() -> Baseline {
    Baseline {
        version: 1,
        account_id: "synthetic".into(),
        source: "library".into(),
        complete: true,
        ordering: Ordering::AcquiredNewestFirst,
        rows: (0..10000).map(row).collect(),
        full_traversal_at: "previous-full-pass".into(),
        details: BTreeSet::new(),
        updated_head: vec![],
    }
}
fn page(rows: Vec<Row>) -> Page {
    Page {
        source: "library".into(),
        route: "/library?page=1".into(),
        rows,
        next: vec![],
        details: vec![],
        terminal: false,
        unresolved: 0,
        fingerprint: "synthetic".into(),
        is_detail: false,
        ordering: Ordering::AcquiredNewestFirst,
        free_item: None,
    }
}
#[test]
fn known_receipts_come_only_from_a_complete_purchase_pass() {
    let receipt = |id: usize, order: &str| Row {
        order_id: Some(order.into()),
        ..row(id)
    };
    let mut pass = Baseline {
        version: 1,
        account_id: "synthetic".into(),
        source: "purchase".into(),
        complete: true,
        ordering: Ordering::Unknown,
        rows: vec![
            receipt(1, "910001"),
            receipt(2, "910001"),
            receipt(3, "910002"),
        ],
        full_traversal_at: "previous-full-pass".into(),
        details: BTreeSet::from(["/orders/910003".into()]),
        updated_head: vec![],
    };
    let known = pass.known_receipts("synthetic", "purchase");
    assert_eq!(
        known.keys().collect::<Vec<_>>(),
        ["/orders/910001", "/orders/910002", "/orders/910003"]
    );
    assert_eq!(known["/orders/910001"].len(), 2);
    assert!(known["/orders/910003"].is_empty());
    assert!(pass.known_receipts("other", "purchase").is_empty());
    pass.complete = false;
    assert!(pass.known_receipts("synthetic", "purchase").is_empty());
    pass.complete = true;
    pass.rows[0].error_code = Some("DIGITAL_CONTROLS_UNRESOLVED".into());
    assert!(pass.known_receipts("synthetic", "purchase").is_empty());
    pass.rows[0].error_code = None;
    pass.rows[2].order_id = None;
    assert!(pass.known_receipts("synthetic", "purchase").is_empty());
    let mut library = base();
    library.details = BTreeSet::from(["/orders/910003".into()]);
    assert!(library.known_receipts("synthetic", "library").is_empty());
    // Baselines saved before receipt routes were recorded still load.
    let mut saved = serde_json::to_value(base()).unwrap();
    saved.as_object_mut().unwrap().remove("details");
    assert!(serde_json::from_value::<Baseline>(saved)
        .unwrap()
        .details
        .is_empty());
}
#[test]
fn moved_page_boundary_stops_by_stable_records_and_preserves_tail() {
    let b = base();
    let mut t = Tracker::new(&b);
    let first = page(vec![row(20000), row(20001), row(0)]);
    assert_eq!(t.observe(&first), None);
    let second = page(vec![row(1), row(2), row(3)]);
    assert_eq!(t.observe(&second), Some(3));
    let prefix = [first.rows, second.rows].concat();
    let all = merged_rows(&b, &prefix, 3, &[]).unwrap().rows;
    assert_eq!(all.len(), 10002);
    assert_eq!(all.last().unwrap().source_record_id, "synthetic:9999");
}
#[test]
fn arbitrarily_long_new_prefix_has_no_fixed_page_stop() {
    let mut t = Tracker::new(&base());
    for i in 0..150 {
        assert_eq!(t.observe(&page(vec![row(20000 + i)])), None);
    }
    assert_eq!(t.observe(&page(vec![row(0), row(1)])), None);
    assert_eq!(t.observe(&page(vec![row(2), row(3)])), Some(3));
}
#[test]
fn changed_known_file_resets_two_page_overlap() {
    let mut t = Tracker::new(&base());
    assert_eq!(t.observe(&page(vec![row(0), row(1)])), None);
    let mut changed = row(2);
    changed.files.push(super::super::parser::File {
        id: "700001".into(),
        variant_id: Some("800001".into()),
        name: Some("new-file.zip".into()),
        size_bytes: Some(10),
        source_url: "https://booth.pm/downloadables/700001".into(),
        revision: "source:changed".into(),
        format: None,
    });
    assert_eq!(t.observe(&page(vec![changed, row(3)])), None);
    assert_eq!(t.observe(&page(vec![row(4), row(5)])), Some(5));
}
#[test]
fn unknown_order_partial_baseline_and_interleaving_never_shortcut() {
    let mut b = base();
    b.complete = false;
    assert!(!b.usable("synthetic", "library"));
    assert_eq!(Tracker::new(&b).observe(&page(vec![row(0)])), None);
    b.complete = true;
    for rows in [
        vec![row(1), row(0)],
        vec![row(0), row(20000)],
        vec![row(0), row(0)],
    ] {
        let mut t = Tracker::new(&b);
        assert_eq!(t.observe(&page(rows)), None);
        assert_eq!(t.observe(&page(vec![row(2), row(3)])), None);
    }
    let mut t = Tracker::new(&b);
    let mut p = page(vec![row(0)]);
    p.ordering = Ordering::Unknown;
    assert_eq!(t.observe(&p), None);
    assert_eq!(t.observe(&page(vec![row(1), row(2)])), None);
}
#[test]
fn resumed_prefix_reconstructs_proof_without_page_number_assumptions() {
    let b = base();
    let prefix = page(vec![row(20000), row(0)]);
    let mut resumed = Tracker::new(&b);
    assert_eq!(resumed.observe(&prefix), None);
    assert_eq!(resumed.observe(&page(vec![row(1), row(2)])), Some(2));
}
fn updated(rows: Vec<Row>) -> Page {
    Page {
        ordering: Ordering::FileUpdatedNewestFirst,
        next: vec![safety::updated_route(9)],
        ..page(rows)
    }
}
fn with_file(id: usize, file: &str) -> Row {
    let mut changed = row(id);
    changed.files.push(super::super::parser::File {
        id: file.into(),
        variant_id: None,
        name: Some("new-file.zip".into()),
        size_bytes: None,
        source_url: format!("https://booth.pm/downloadables/{file}"),
        revision: "unknown".into(),
        format: None,
    });
    changed
}
fn overlap(base: &Baseline, pages: Vec<Vec<Row>>) -> Tracker {
    let mut tracker = Tracker::new(base);
    let last = pages.len() - 1;
    for (index, rows) in pages.into_iter().enumerate() {
        let proof = tracker.observe(&page(rows));
        assert_eq!(proof.is_some(), index == last);
    }
    tracker
}
#[test]
fn a_product_listed_twice_is_matched_by_position() {
    // Two rows of one product, whatever page each falls on.
    let mut b = base();
    b.rows[5] = with_file(1, "700001");
    assert!(b.usable("synthetic", "library"));
    let same_order = [
        row(0),
        row(1),
        row(2),
        row(3),
        row(4),
        with_file(1, "700001"),
        row(6),
    ];
    assert_eq!(
        Tracker::new(&b).observe(&page(same_order[..4].to_vec())),
        None
    );
    let mut t = Tracker::new(&b);
    assert_eq!(t.observe(&page(same_order[..3].to_vec())), None);
    assert_eq!(t.observe(&page(same_order[3..].to_vec())), Some(6));
    // A row with one place cannot appear twice.
    let mut t = Tracker::new(&b);
    assert_eq!(t.observe(&page(vec![row(0), row(0)])), None);
    assert_eq!(t.observe(&page(vec![row(1), row(2)])), None);
    // An earlier place never answers for a later row.
    let mut t = Tracker::new(&b);
    assert_eq!(t.observe(&page(vec![with_file(1, "700001"), row(1)])), None);
    assert_eq!(t.observe(&page(vec![row(6), row(7)])), None);
    // The suffix a row takes from sharing a page says nothing about the row.
    let mut split = row(1);
    split.source_record_id = "synthetic:1:occurrence:1".into();
    let mut t = Tracker::new(&base());
    assert_eq!(t.observe(&page(vec![row(0), split])), None);
    assert_eq!(t.observe(&page(vec![row(2)])), Some(2));
}
#[test]
fn a_delisted_row_neither_blocks_the_overlap_nor_leaves_a_gap() {
    let delisted = |route: &str| Row {
        source_record_id: format!("library:{route}"),
        unavailable: true,
        error_code: Some("BOOTH_WITHHELD_PRODUCT_ID".into()),
        ..Row::default()
    };
    let mut b = base();
    b.rows[2] = delisted("old-place");
    b.rows[5] = delisted("other-old-place");
    assert!(b.usable("synthetic", "library"));
    let mut t = Tracker::new(&b);
    assert_eq!(
        t.observe(&page(vec![row(0), row(1), delisted("new-place")])),
        None
    );
    assert_eq!(t.observe(&page(vec![row(3), row(4)])), Some(4));
    // It stays out of what is carried forward, or it would be there again next time.
    let prefix = [row(0), row(1), delisted("new-place"), row(3), row(4)];
    let all = merged_rows(&b, &prefix, 4, &[]).unwrap().rows;
    assert_eq!(all.iter().filter(|row| withheld(row)).count(), 1);
    let mut other = base();
    other.rows[2].error_code = Some("PRODUCT_ID_UNRESOLVED".into());
    assert!(!other.usable("synthetic", "library"));
}
#[test]
fn updated_order_settles_after_two_quiet_pages_and_keeps_changes() {
    let b = base();
    // A new purchase, then the overlap; the updated order shows the rest.
    let t = overlap(&b, vec![vec![row(20000), row(0)], vec![row(1), row(2)]]);
    let mut u = t.updated(&[]);
    assert!(u.changes(&with_file(9000, "700001")));
    assert!(!u.changes(&row(20000)) && !u.changes(&row(9000)));
    // A new file on a row far down the list, and the new purchase, then two quiet pages.
    assert_eq!(
        u.observe(&updated(vec![with_file(9000, "700001"), row(20000)])),
        UpdatedRead::Reading
    );
    assert_eq!(
        u.observe(&updated(vec![row(5000), row(5001)])),
        UpdatedRead::Reading
    );
    assert_eq!(
        u.observe(&updated(vec![row(4000), row(3000)])),
        UpdatedRead::Settled
    );
    let prefix = [row(20000), row(0), row(1), row(2)];
    let read = [with_file(9000, "700001"), row(20000), row(5000)];
    let merged = merged_rows(&b, &prefix, 2, &read).unwrap();
    assert_eq!(merged.changed.len(), 1);
    let all = merged.rows;
    assert_eq!(all.len(), 10001);
    assert_eq!(all[0].source_record_id, "synthetic:20000");
    assert_eq!(
        all.iter()
            .filter(|r| r.source_record_id == "synthetic:9000")
            .count(),
        1
    );
    let place = all
        .iter()
        .position(|r| r.source_record_id == "synthetic:9000")
        .unwrap();
    assert_eq!(all[place].files.len(), 1);
    assert_eq!(place, 9000 + 1);
    // A change resets the count, a new purchase does not count, and neither does a page
    // of delisted rows, which says nothing.
    let mut u = t.updated(&[]);
    assert_eq!(u.observe(&updated(vec![row(7000)])), UpdatedRead::Reading);
    assert_eq!(
        u.observe(&updated(vec![with_file(7001, "700002")])),
        UpdatedRead::Reading
    );
    assert_eq!(u.observe(&updated(vec![row(7002)])), UpdatedRead::Reading);
    assert_eq!(u.observe(&updated(vec![row(20000)])), UpdatedRead::Reading);
    assert_eq!(u.observe(&updated(vec![row(7003)])), UpdatedRead::Reading);
    let delisted = Row {
        source_record_id: "library:delisted".into(),
        error_code: Some("BOOTH_WITHHELD_PRODUCT_ID".into()),
        ..Row::default()
    };
    assert_eq!(u.observe(&updated(vec![delisted])), UpdatedRead::Reading);
    assert_eq!(u.observe(&updated(vec![row(7004)])), UpdatedRead::Settled);
}
#[test]
fn updated_order_reads_past_rows_moved_without_a_visible_change() {
    let b = base();
    let t = overlap(&b, vec![vec![row(0)], vec![row(1)]]);
    let head: Vec<_> = (100..140).map(|id| content(&row(id))).collect();
    let leading = |from: usize, to: usize| (from..to).map(row).collect::<Vec<_>>();
    // Thirty rows BOOTH moved show nothing new here, then a real change, then the old order.
    let mut u = t.updated(&head);
    for start in [500, 510, 520] {
        assert_eq!(
            u.observe(&updated(leading(start, start + 10))),
            UpdatedRead::Reading,
            "{start}"
        );
    }
    assert_eq!(
        u.observe(&updated(vec![
            with_file(9000, "700001"),
            row(100),
            row(101)
        ])),
        UpdatedRead::Reading
    );
    // A row of the old order that moved is simply absent from its place.
    assert_eq!(
        u.observe(&updated(vec![
            row(102),
            row(103),
            row(105),
            row(106),
            row(107),
            row(108),
            row(109)
        ])),
        UpdatedRead::Reading
    );
    assert_eq!(u.observe(&updated(vec![row(110)])), UpdatedRead::Settled);
    // Rows out of the old order start the count again.
    let mut u = t.updated(&head);
    let mut scrambled = leading(100, 105);
    scrambled.reverse();
    assert_eq!(u.observe(&updated(scrambled)), UpdatedRead::Reading);
    assert_eq!(u.observe(&updated(leading(105, 113))), UpdatedRead::Reading);
    assert_eq!(u.observe(&updated(leading(113, 115))), UpdatedRead::Settled);
    // A row of the old order with new files is a change, not a witness.
    let mut u = t.updated(&head);
    assert_eq!(u.observe(&updated(leading(100, 109))), UpdatedRead::Reading);
    assert_eq!(
        u.observe(&updated(vec![with_file(109, "700002")])),
        UpdatedRead::Reading
    );
    assert_eq!(u.observe(&updated(leading(110, 119))), UpdatedRead::Reading);
    assert_eq!(u.observe(&updated(leading(119, 120))), UpdatedRead::Settled);
    // The next pass looks for what was read now, then what fell below it, within a bound.
    let read = leading(500, 503);
    let next = next_head(&read, &head);
    assert_eq!(
        next[..3],
        [content(&read[0]), content(&read[1]), content(&read[2])]
    );
    assert_eq!(next[3], head[0]);
    assert_eq!(next.len(), HEAD_KEPT);
    assert_eq!(next_head(&[], &head), head);
}
#[test]
fn updated_order_gives_up_when_anything_does_not_fit() {
    let b = base();
    let t = overlap(&b, vec![vec![row(0)], vec![row(1)]]);
    let unknown = vec![row(20000)];
    let mut unstated = updated(vec![row(5)]);
    unstated.ordering = Ordering::AcquiredNewestFirst;
    let mut unresolved = updated(vec![row(5)]);
    unresolved.unresolved = 1;
    let mut unstable = row(5);
    unstable.error_code = Some("PRODUCT_TITLE_UNRESOLVED".into());
    let mut dead_end = updated(vec![row(5)]);
    dead_end.next.clear();
    for bad in [
        updated(unknown),
        unstated,
        unresolved,
        updated(vec![unstable]),
        updated(vec![]),
        dead_end,
    ] {
        assert_eq!(t.updated(&[]).observe(&bad), UpdatedRead::Broken);
    }
    // The list's own end is not a dead end, and a page that settles needs no next page.
    let mut last = updated(vec![row(5)]);
    last.next.clear();
    last.terminal = true;
    assert_eq!(t.updated(&[]).observe(&last), UpdatedRead::Reading);
    let head = [content(&row(5))];
    let mut settling = updated(vec![row(5)]);
    settling.next.clear();
    assert_eq!(t.updated(&head).observe(&settling), UpdatedRead::Settled);
    // Two different rows of one product: a new file cannot be pinned on either.
    let mut twice = base();
    twice.rows[5] = with_file(1, "700001");
    let t = overlap(&twice, vec![vec![row(0)], vec![row(1)]]);
    assert_eq!(
        t.updated(&[])
            .observe(&updated(vec![with_file(1, "700002")])),
        UpdatedRead::Broken
    );
    assert_eq!(
        t.updated(&[])
            .observe(&updated(vec![with_file(1, "700001")])),
        UpdatedRead::Reading
    );
    // Nor on either of two identical rows.
    let mut same = base();
    same.rows[5] = row(1);
    let t = overlap(&same, vec![vec![row(0)], vec![row(1)]]);
    assert_eq!(
        t.updated(&[])
            .observe(&updated(vec![with_file(1, "700002")])),
        UpdatedRead::Broken
    );
}
#[test]
fn a_row_keeps_its_files_whichever_record_id_its_page_gives_it() {
    // A product bought twice with different files, listed together in one order and apart
    // in the other, so their record IDs differ between the two.
    let mut b = base();
    let mut second = with_file(1, "700009");
    second.source_record_id = "synthetic:1:occurrence:1".into();
    b.rows[2] = second;
    let first = with_file(1, "700001");
    b.rows[1] = first.clone();
    let t = overlap(&b, vec![vec![row(0)], vec![first.clone()]]);
    let mut apart = with_file(1, "700009");
    apart.source_record_id = "synthetic:1".into();
    let mut u = t.updated(&[]);
    assert_eq!(
        u.observe(&updated(vec![first.clone()])),
        UpdatedRead::Reading
    );
    assert_eq!(
        u.observe(&updated(vec![apart.clone()])),
        UpdatedRead::Settled
    );
    let merged = merged_rows(&b, &[row(0), first], 1, &[apart]).unwrap();
    assert!(merged.changed.is_empty());
    let names: Vec<_> = merged.rows[..3]
        .iter()
        .map(|row| row.files.first().map(|file| file.id.clone()))
        .collect();
    assert_eq!(
        names,
        [None, Some("700001".to_owned()), Some("700009".to_owned())]
    );
}
#[test]
fn only_an_order_the_page_stated_and_a_recent_full_read_allow_stopping() {
    let mut b = base();
    for ordering in [
        Ordering::Unknown,
        Ordering::FileUpdatedNewestFirst,
        Ordering::DownloadedNewestFirst,
    ] {
        b.ordering = ordering;
        assert!(!b.usable("synthetic", "library"), "{ordering:?}");
    }
    let now = chrono::DateTime::parse_from_rfc3339("2026-11-05T12:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    for (at, recent) in [
        ("2026-11-05T11:00:00+00:00", true),
        ("2026-10-30T12:00:01+00:00", true),
        ("2026-10-29T12:00:00+00:00", false),
        ("2026-11-05T12:00:00+00:00", true),
        ("2026-11-05T12:00:01+00:00", false),
        ("2027-01-01T00:00:00+00:00", false),
        ("previous-full-pass", false),
        ("", false),
    ] {
        b.full_traversal_at = at.into();
        assert_eq!(b.recently_read_in_full(now), recent, "{at}");
    }
}
