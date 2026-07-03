use crate::db::Database;

#[test]
fn sqlite_vec_is_registered_and_knn_works() {
    let db = Database::open_in_memory().unwrap();
    // vec_version() is only callable if sqlite-vec is registered on this connection.
    let v: String = db
        .conn
        .query_row("SELECT vec_version()", [], |r| r.get(0))
        .unwrap();
    assert!(v.starts_with('v'), "unexpected vec_version: {v}");

    db.conn
        .execute_batch("CREATE VIRTUAL TABLE temp.t USING vec0(embedding float[3]);")
        .unwrap();
    let blob = |v: &[f32]| crate::embed::vec_to_blob(v);
    db.conn
        .execute(
            "INSERT INTO temp.t(rowid, embedding) VALUES (1, ?1)",
            [blob(&[1.0, 0.0, 0.0])],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO temp.t(rowid, embedding) VALUES (2, ?1)",
            [blob(&[0.0, 1.0, 0.0])],
        )
        .unwrap();
    let top: i64 = db
        .conn
        .query_row(
            "SELECT rowid FROM temp.t WHERE embedding MATCH ?1 ORDER BY distance LIMIT 1",
            [blob(&[0.9, 0.1, 0.0])],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(top, 1); // nearest to [0.9,0.1,0] is [1,0,0]
}

#[test]
fn upsert_embedding_populates_vec_index_when_enabled() {
    let db = Database::open_in_memory_vec().unwrap();
    let o = db
        .save_observation(
            "t",
            "alpha beta",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    db.upsert_embedding("observation", o.id, "hash-v1", &[0.1, 0.2, 0.3])
        .unwrap();
    // the vec0 table now has a row for this observation
    let n: i64 = db
        .conn
        .query_row("SELECT count(*) FROM embeddings_vec", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
    // meta records dim + model
    let (dim, model): (i64, String) = db
        .conn
        .query_row("SELECT dim, model FROM vec_index_meta LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(dim, 3);
    assert_eq!(model, "hash-v1");
}

#[test]
fn vector_search_via_vec_index_matches_bruteforce_top() {
    use crate::embed::{Embedder, HashEmbedder};
    let e = HashEmbedder::new(64);

    // Brute-force DB
    let brute = Database::open_in_memory().unwrap();
    // Vec-index DB
    let vec = Database::open_in_memory_vec().unwrap();

    for db in [&brute, &vec] {
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
    }

    let qe = e.embed("alpha beta").unwrap();
    let b = brute.vector_search(&qe, e.model(), None, None, 10).unwrap();
    let v = vec.vector_search(&qe, e.model(), None, None, 10).unwrap();
    // same top observation via either backend
    assert_eq!(b[0].0.title, "near");
    assert_eq!(v[0].0.title, "near");
    // vec backend also excludes soft-deleted
    vec.delete_observation(v[0].0.id).unwrap();
    let v2 = vec.vector_search(&qe, e.model(), None, None, 10).unwrap();
    assert!(v2.iter().all(|(o, _)| o.title != "near"));
}
