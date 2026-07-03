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

#[test]
fn vec_index_rebuild_repopulates_from_embeddings() {
    let db = Database::open_in_memory_vec().unwrap();
    let a = db
        .save_observation("a", "one", "manual", None, "project", None, None, None)
        .unwrap();
    let b = db
        .save_observation("b", "two", "manual", None, "project", None, None, None)
        .unwrap();
    // store durable embeddings but simulate a stale index by upserting then dropping the vec table
    db.upsert_embedding("observation", a.id, "hash-v1", &[0.1, 0.2, 0.3])
        .unwrap();
    db.upsert_embedding("observation", b.id, "hash-v1", &[0.4, 0.5, 0.6])
        .unwrap();
    db.conn
        .execute_batch("DROP TABLE IF EXISTS embeddings_vec; DELETE FROM vec_index_meta;")
        .unwrap();

    let n = db.vec_index_rebuild("hash-v1").unwrap();
    assert_eq!(n, 2);
    let count: i64 = db
        .conn
        .query_row("SELECT count(*) FROM embeddings_vec", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

/// Proves that `vector_search` actually takes the vec0 branch (not the
/// brute-force fallback) when the vector index is enabled: it JOINs against
/// `embeddings_vec` (via the `knn` CTE), so an observation with a durable
/// embedding that was never pushed into the vec0 table (bypassing
/// `upsert_embedding`) structurally cannot be returned. A brute-force DB with
/// the same durable embeddings *does* return it — showing the two paths are
/// genuinely different, not just result-identical by coincidence.
#[test]
fn vec_index_search_only_sees_vectors_present_in_the_vec0_table() {
    use crate::embed::{Embedder, HashEmbedder, vec_to_blob};

    let e = HashEmbedder::new(64);
    let model = e.model();

    let vec_db = Database::open_in_memory_vec().unwrap();
    let brute_db = Database::open_in_memory().unwrap();

    // X: goes through upsert_embedding, so it lands in BOTH the durable
    // `embeddings` table and `embeddings_vec`.
    let x_vec = vec_db
        .save_observation(
            "x",
            "alpha beta",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let x_brute = brute_db
        .save_observation(
            "x",
            "alpha beta",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let x_embedding = e.embed(&x_vec.content).unwrap();
    vec_db
        .upsert_embedding("observation", x_vec.id, model, &x_embedding)
        .unwrap();
    brute_db
        .upsert_embedding("observation", x_brute.id, model, &x_embedding)
        .unwrap();

    // Y: durable embedding inserted directly via raw SQL, bypassing
    // upsert_embedding — present in `embeddings` but absent from
    // `embeddings_vec`. Engineered to be the query's nearest neighbor.
    let y_vec = vec_db
        .save_observation(
            "y",
            "delta gamma",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let y_brute = brute_db
        .save_observation(
            "y",
            "delta gamma",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let y_embedding = e.embed(&y_vec.content).unwrap();
    let y_blob = vec_to_blob(&y_embedding);
    let y_dim = y_embedding.len() as i64;
    for (db, y_id) in [(&vec_db, y_vec.id), (&brute_db, y_brute.id)] {
        db.conn
            .execute(
                "INSERT INTO embeddings(object_type, object_id, model, dim, vector, created_at)
                 VALUES ('observation', ?1, ?2, ?3, ?4, '2026-01-01T00:00:00Z')",
                rusqlite::params![y_id, model, y_dim, y_blob],
            )
            .unwrap();
    }

    // Query with Y's own embedding — its nearest match by cosine similarity.
    let query = y_embedding.clone();

    let brute_results = brute_db
        .vector_search(&query, model, None, None, 10)
        .unwrap();
    assert_eq!(
        brute_results[0].0.title, "y",
        "brute-force search (over the durable table) must find Y as nearest"
    );

    let vec_results = vec_db.vector_search(&query, model, None, None, 10).unwrap();
    assert!(
        vec_results.iter().all(|(o, _)| o.title != "y"),
        "vec0-backed search must never return Y — it is absent from embeddings_vec, \
         which proves the vec0 branch (not the brute-force fallback) executed"
    );
    assert_eq!(
        vec_results.len(),
        1,
        "only X (present in embeddings_vec) can be returned"
    );
    assert_eq!(vec_results[0].0.title, "x");
}
