use crate::db::Database;

#[test]
fn schema_v3_creates_embeddings_table() {
    let db = Database::open_in_memory().unwrap();
    let n: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='embeddings'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
    let version: u32 = db
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 3);
}

#[test]
fn hash_embedder_is_deterministic_and_normalized() {
    use crate::embed::{Embedder, HashEmbedder};
    let e = HashEmbedder::new(64);
    let a = e.embed("alpha beta gamma").unwrap();
    let b = e.embed("alpha beta gamma").unwrap();
    assert_eq!(a, b); // deterministic
    assert_eq!(a.len(), 64);
    let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-4 || norm == 0.0);
    assert_eq!(e.model(), "hash-v1");
}

#[test]
fn vector_blob_roundtrip_and_cosine() {
    use crate::embed::{blob_to_vec, cosine_similarity, vec_to_blob};
    let v = vec![0.1f32, -0.2, 0.3, 0.4];
    let round = blob_to_vec(&vec_to_blob(&v));
    assert_eq!(v, round);
    // identical vectors → cosine 1.0; orthogonal → 0.0
    assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-5);
    assert!((cosine_similarity(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
}

#[test]
fn upsert_embedding_roundtrips_and_replaces() {
    use crate::embed::blob_to_vec;
    let db = Database::open_in_memory().unwrap();
    let obs = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();

    db.upsert_embedding("observation", obs.id, "hash-v1", &[0.1, 0.2, 0.3])
        .unwrap();
    // one row, correct dim + roundtrip
    let (dim, blob): (i64, Vec<u8>) = db
        .conn
        .query_row(
            "SELECT dim, vector FROM embeddings WHERE object_id = ?1 AND model = 'hash-v1'",
            [obs.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(dim, 3);
    assert_eq!(blob_to_vec(&blob), vec![0.1f32, 0.2, 0.3]);

    // upsert same key replaces (no duplicate)
    db.upsert_embedding("observation", obs.id, "hash-v1", &[0.9, 0.8])
        .unwrap();
    let count: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM embeddings WHERE object_id = ?1 AND model = 'hash-v1'",
            [obs.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn vector_search_ranks_by_cosine() {
    use crate::embed::{Embedder, HashEmbedder};
    let db = Database::open_in_memory().unwrap();
    let e = HashEmbedder::new(64);

    let near = db
        .save_observation(
            "near",
            "alpha beta gamma",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let far = db
        .save_observation(
            "far",
            "delta epsilon zeta",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    for o in [&near, &far] {
        db.upsert_embedding(
            "observation",
            o.id,
            e.model(),
            &e.embed(&o.content).unwrap(),
        )
        .unwrap();
    }

    // query shares tokens with `near`
    let qe = e.embed("alpha beta").unwrap();
    let hits = db.vector_search(&qe, e.model(), None, None, 10).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].0.id, near.id); // highest cosine first
    assert!(hits[0].1 >= hits[1].1);
    // soft-deleted observations are excluded
    db.delete_observation(near.id).unwrap();
    let hits2 = db.vector_search(&qe, e.model(), None, None, 10).unwrap();
    assert_eq!(hits2.len(), 1);
    assert_eq!(hits2[0].0.id, far.id);
}
