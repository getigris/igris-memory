use crate::db::Database;

#[test]
fn parse_ollama_embedding_response() {
    use crate::embed::parse_embedding_response;
    let ok = r#"{"embedding":[0.1,0.2,0.3]}"#;
    assert_eq!(
        parse_embedding_response(ok).unwrap(),
        vec![0.1f32, 0.2, 0.3]
    );
    assert!(parse_embedding_response(r#"{"nope":1}"#).is_err()); // missing field
    assert!(parse_embedding_response(r#"{"embedding":[]}"#).is_err()); // empty
    assert!(parse_embedding_response("not json").is_err());
}

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
    assert_eq!(version, 4);
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

#[test]
fn hybrid_search_without_embedding_equals_fts() {
    let db = Database::open_in_memory().unwrap();
    db.save_observation(
        "a",
        "alpha keyword one",
        "manual",
        None,
        "project",
        None,
        None,
        None,
    )
    .unwrap();
    db.save_observation(
        "b",
        "alpha keyword two",
        "manual",
        None,
        "project",
        None,
        None,
        None,
    )
    .unwrap();

    let fts = db.search("keyword", None, None, Some(10)).unwrap();
    let hybrid = db
        .hybrid_search("keyword", None, "hash-v1", None, None, Some(10))
        .unwrap();
    let fts_ids: Vec<i64> = fts.iter().map(|r| r.observation.id).collect();
    let hyb_ids: Vec<i64> = hybrid.iter().map(|r| r.observation.id).collect();
    assert_eq!(fts_ids, hyb_ids);
}

#[test]
fn hybrid_search_fuses_vector_hits() {
    use crate::embed::{Embedder, HashEmbedder};
    let db = Database::open_in_memory().unwrap();
    let e = HashEmbedder::new(64);
    // `only_vec` does NOT contain the FTS query word "keyword", but its embedding
    // overlaps the query tokens — it must surface via the vector arm.
    let only_vec = db
        .save_observation(
            "v",
            "alpha beta gamma",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let fts_hit = db
        .save_observation(
            "f",
            "keyword alpha",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    for o in [&only_vec, &fts_hit] {
        db.upsert_embedding(
            "observation",
            o.id,
            e.model(),
            &e.embed(&o.content).unwrap(),
        )
        .unwrap();
    }
    let qe = e.embed("keyword alpha beta").unwrap();
    let hybrid = db
        .hybrid_search("keyword", Some(&qe), e.model(), None, None, Some(10))
        .unwrap();
    let ids: Vec<i64> = hybrid.iter().map(|r| r.observation.id).collect();
    // both surface: fts_hit via FTS, only_vec via the vector arm (FTS alone would miss it)
    assert!(ids.contains(&only_vec.id));
    assert!(ids.contains(&fts_hit.id));
    // fused rank score is populated (higher = better) and sorted descending
    assert!(hybrid[0].rank >= hybrid[hybrid.len() - 1].rank);
}

#[test]
fn observations_needing_embedding_excludes_embedded() {
    let db = Database::open_in_memory().unwrap();
    let a = db
        .save_observation("a", "one", "manual", None, "project", None, None, None)
        .unwrap();
    let b = db
        .save_observation("b", "two", "manual", None, "project", None, None, None)
        .unwrap();
    // embed only `a`
    db.upsert_embedding("observation", a.id, "hash-v1", &[0.1, 0.2])
        .unwrap();

    let need = db.observations_needing_embedding("hash-v1").unwrap();
    assert_eq!(need.len(), 1);
    assert_eq!(need[0].0, b.id);
    assert_eq!(need[0].1, "two");
}

#[test]
fn server_with_embedder_embeds_on_save_and_search_hybrid() {
    use crate::embed::HashEmbedder;
    use crate::server::IgrisServer;
    use crate::server::args::{SaveArgs, SearchArgs};
    use rmcp::handler::server::wrapper::Parameters;
    use std::sync::Arc;

    let db = Database::open_in_memory().unwrap();
    let server = IgrisServer::with_embedder(db, Some(Arc::new(HashEmbedder::new(64))));

    // save → an embedding row is created for this observation under model "hash-v1"
    let save_json = server.igris_save(Parameters(SaveArgs {
        title: "t".into(),
        content: "alpha beta gamma".into(),
        observation_type: "manual".into(),
        project: None,
        scope: "project".into(),
        topic_key: None,
        tags: None,
        session_id: None,
        mentions: None,
    }));
    assert!(!save_json.contains("\"error\""), "save failed: {save_json}");

    // search returns a non-error result (hybrid path exercised)
    let search_json = server.igris_search(Parameters(SearchArgs {
        query: "alpha".into(),
        observation_type: None,
        project: None,
        limit: None,
    }));
    assert!(
        !search_json.contains("\"error\""),
        "search failed: {search_json}"
    );
    assert!(search_json.contains("alpha")); // the saved content surfaces
}
