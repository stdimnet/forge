//! Property test: 2 to 5 replicas edit concurrently and their updates reach the host's
//! document and the other replicas at random times, and everyone ends with the same text.
//! As on the wire, each replica receives updates in the order the host forwarded them (one
//! ordered stream per participant); what varies is how edits interleave with deliveries.

use forge_collab::docs::SharedDoc;
use forge_collab::protocol::DocId;
use proptest::prelude::*;
use yrs::updates::decoder::Decode;
use yrs::{Doc, GetString, OffsetKind, Options, ReadTxn, Text, TextRef, Transact, Update};

fn replica(state: &[u8]) -> (Doc, TextRef) {
    let d = Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Options::default()
    });
    let t = d.get_or_insert_text("content");
    d.transact_mut()
        .apply_update(Update::decode_v1(state).unwrap())
        .unwrap();
    (d, t)
}

fn apply(d: &Doc, u: &[u8]) {
    d.transact_mut()
        .apply_update(Update::decode_v1(u).unwrap())
        .unwrap();
}

#[derive(Debug, Clone)]
struct Op {
    peer: usize,
    /// Character position as a fraction of the text (stays valid as texts change).
    at: u8,
    delete: u8,
    insert: &'static str,
}

fn op(peers: usize) -> impl Strategy<Value = Op> {
    (
        0..peers,
        any::<u8>(),
        0u8..3,
        prop::sample::select(vec!["x", "ü", "😀", "\n", "ab", "\r\n", ""]),
    )
        .prop_map(|(peer, at, delete, insert)| Op {
            peer,
            at,
            delete,
            insert,
        })
}

fn case() -> impl Strategy<Value = (usize, Vec<Op>, Vec<u64>)> {
    (2usize..=5).prop_flat_map(|peers| {
        (
            Just(peers),
            prop::collection::vec(op(peers), 1..40),
            prop::collection::vec(any::<u64>(), 40),
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn replicas_converge_with_any_interleaving((peers, ops, shuffle) in case()) {
        let mut host = SharedDoc::new(DocId(1), "a.txt".into(), "hello\nworld\n", None);
        let start = host.full_update();
        let replicas: Vec<(Doc, TextRef)> = (0..peers).map(|_| replica(&start)).collect();
        // Updates in flight per replica, oldest first; the host gets every update at once.
        let mut in_flight: Vec<std::collections::VecDeque<Vec<u8>>> = vec![Default::default(); peers];
        for (i, o) in ops.iter().enumerate() {
            let (d, t) = &replicas[o.peer];
            let text = t.get_string(&d.transact());
            let bounds: Vec<u32> = std::iter::once(0)
                .chain(text.chars().scan(0u32, |n, c| { *n += c.len_utf16() as u32; Some(*n) }))
                .collect();
            let s = o.at as usize * (bounds.len() - 1) / 255;
            let e = (s + o.delete as usize).min(bounds.len() - 1);
            let before = d.transact().state_vector();
            {
                let mut txn = d.transact_mut();
                if e > s { t.remove_range(&mut txn, bounds[s], bounds[e] - bounds[s]); }
                if !o.insert.is_empty() { t.insert(&mut txn, bounds[s], o.insert); }
            }
            let u = d.transact().encode_state_as_update_v1(&before);
            host.apply_update(&u).unwrap();
            for p in (0..peers).filter(|p| *p != o.peer) {
                in_flight[p].push_back(u.clone());
            }
            // Some replicas catch up now, some later.
            let r = shuffle[i % shuffle.len()];
            for (p, queue) in in_flight.iter_mut().enumerate() {
                let n = ((r >> (p * 4)) % 4) as usize;
                for u in queue.drain(..n.min(queue.len())) {
                    apply(&replicas[p].0, &u);
                }
            }
        }
        for (p, queue) in in_flight.iter_mut().enumerate() {
            for u in queue.drain(..) {
                apply(&replicas[p].0, &u);
            }
        }
        let expected = host.text();
        for (d, t) in &replicas {
            prop_assert_eq!(t.get_string(&d.transact()), expected.clone());
        }
    }
}
