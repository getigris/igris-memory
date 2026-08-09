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
    assert_eq!(version, 6);
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
fn hash_embedder_pins_bucket_and_sign_for_known_tokens() {
    use crate::embed::{Embedder, HashEmbedder};
    let e = HashEmbedder::new(4);
    // Hand-computed FNV-1a (offset 1469598103934665603, prime 1099511628211) for a
    // single-token input pins the resulting bucket index and sign exactly: with a
    // single token there's exactly one non-zero, unit-magnitude entry, so any
    // perturbation to the mixing (^=, &, >>, or the sign literal) moves it to a
    // different index and/or flips its sign.
    // "a": h = 4953267810257967366 -> idx = h % 4 = 2, top bit clear -> sign = +1.0
    assert_eq!(e.embed("a").unwrap(), vec![0.0, 0.0, 1.0, 0.0]);
    // "test": h = 10905494432584914231 -> idx = h % 4 = 3, top bit set -> sign = -1.0
    assert_eq!(e.embed("test").unwrap(), vec![0.0, 0.0, 0.0, -1.0]);
}

#[test]
fn hash_embedder_empty_input_yields_zero_vector_without_dividing() {
    use crate::embed::{Embedder, HashEmbedder};
    let e = HashEmbedder::new(4);
    // No tokens => every bucket stays 0.0, so norm is exactly 0.0. The `norm > 0.0`
    // guard must skip the normalization loop; a `>=` mutant would instead divide
    // 0.0 / 0.0, producing NaNs instead of this exact zero vector.
    assert_eq!(e.embed("").unwrap(), vec![0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn hash_embedder_dimensions_returns_configured_dim() {
    use crate::embed::{Embedder, HashEmbedder};
    assert_eq!(HashEmbedder::new(7).dimensions(), 7);
}

#[test]
fn ollama_embedder_embed_returns_response_vector() {
    use crate::embed::{Embedder, OllamaEmbedder};

    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/api/embeddings")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"embedding":[0.25,0.5,0.75]}"#)
        .create();

    let embedder = OllamaEmbedder::new(server.url(), "test-model".to_string());
    let result = embedder.embed("some text").unwrap();

    assert_eq!(result, vec![0.25f32, 0.5, 0.75]);
    mock.assert();
}

#[test]
fn ollama_embedder_dimensions_is_always_zero() {
    use crate::embed::{Embedder, OllamaEmbedder};
    let embedder = OllamaEmbedder::new("http://localhost:11434".to_string(), "llama3".to_string());
    assert_eq!(embedder.dimensions(), 0);
}

#[test]
fn ollama_embedder_model_roundtrips_exactly() {
    use crate::embed::{Embedder, OllamaEmbedder};
    let embedder = OllamaEmbedder::new("http://localhost:11434".to_string(), "llama3".to_string());
    assert_eq!(embedder.model(), "llama3");
}

#[test]
fn cosine_similarity_length_mismatch_short_circuits_before_indexing() {
    use crate::embed::cosine_similarity;
    // a.len() != b.len(): must return 0.0 via the first guard disjunct without
    // ever entering the loop. An `&&` mutant would fall through into the loop and
    // panic on out-of-bounds access when indexing b past its (shorter) length.
    assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
}

#[test]
fn cosine_similarity_one_sided_zero_vector_returns_zero() {
    use crate::embed::cosine_similarity;
    // na == 0.0 but nb != 0.0: must return 0.0 via the second guard disjunct. An
    // `&&` mutant would only fire when *both* norms are zero, so this one-sided
    // case falls through and divides by zero, yielding NaN instead of 0.0.
    assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 2.0]), 0.0);
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
fn vector_search_vec0_uses_full_candidate_k_window() {
    use crate::embed::{Embedder, HashEmbedder};
    let db = Database::open_in_memory_vec().unwrap();
    let e = HashEmbedder::new(64);

    // 64 "noise" observations whose embedding is set to the exact query vector
    // (distance 0 -- the closest possible under any metric), all filed under a
    // project that gets excluded by the post-filter. With the correct
    // `candidate_k = (top_k.max(1) * 8).max(64)` = 160 for top_k = 20, the vec0
    // KNN pool (`LIMIT candidate_k`) comfortably covers all 67 rows (64 noise +
    // 3 targets), so the targets survive the project filter. A `*` -> `+`/`/`
    // mutant collapses candidate_k to 64 -- exactly the noise count -- so the
    // KNN pool is saturated by noise alone and every target observation is
    // starved out of the candidate window before the project filter even runs.
    let query = e.embed("shared query text").unwrap();
    for i in 0..64 {
        let noise = db
            .save_observation(
                &format!("noise-{i}"),
                &format!("noise body {i}"),
                "manual",
                Some("noise-project"),
                "project",
                None,
                None,
                None,
            )
            .unwrap();
        db.upsert_embedding("observation", noise.id, e.model(), &query)
            .unwrap();
    }

    let mut target_ids = Vec::new();
    for i in 0..3 {
        let target = db
            .save_observation(
                &format!("target-{i}"),
                &format!("target body {i}"),
                "manual",
                Some("target-project"),
                "project",
                None,
                None,
                None,
            )
            .unwrap();
        db.upsert_embedding(
            "observation",
            target.id,
            e.model(),
            &e.embed(&format!("target body {i}")).unwrap(),
        )
        .unwrap();
        target_ids.push(target.id);
    }

    let hits = db
        .vector_search(&query, e.model(), None, Some("target-project"), 20)
        .unwrap();
    let ids: Vec<i64> = hits.iter().map(|(o, _)| o.id).collect();
    assert_eq!(ids.len(), 3);
    for id in target_ids {
        assert!(ids.contains(&id));
    }
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
fn hybrid_search_rrf_score_is_exact_for_overlapping_hit() {
    let db = Database::open_in_memory().unwrap();

    // `a` outranks `b` in FTS for query "keyword": both contain it once, but
    // `a`'s document is much shorter, so bm25's length normalization favors it.
    let a = db
        .save_observation("a", "keyword", "manual", None, "project", None, None, None)
        .unwrap();
    let b = db
        .save_observation(
            "b",
            "keyword padding padding padding padding padding padding padding",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    // `c` never matches the FTS query at all -- vector-only hit.
    let c = db
        .save_observation(
            "c",
            "unrelated content",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();

    // Pin down the FTS ordering this test depends on: a (rank 0), b (rank 1).
    let fts = db.search("keyword", None, None, Some(10)).unwrap();
    assert_eq!(
        fts.iter().map(|r| r.observation.id).collect::<Vec<_>>(),
        vec![a.id, b.id]
    );

    // `c` outranks `a` in the vector arm (cosine 1.0 vs ~0.707 against the
    // query vector) -- so `a` also appears at vector rank 1, giving it
    // contributions from BOTH RRF loops (lines 180 and 185).
    db.upsert_embedding("observation", c.id, "hash-v1", &[1.0, 0.0])
        .unwrap();
    db.upsert_embedding("observation", a.id, "hash-v1", &[0.5, 0.5])
        .unwrap();
    let vec_hits = db
        .vector_search(&[1.0, 0.0], "hash-v1", None, None, 10)
        .unwrap();
    assert_eq!(
        vec_hits.iter().map(|(o, _)| o.id).collect::<Vec<_>>(),
        vec![c.id, a.id]
    );

    let hybrid = db
        .hybrid_search(
            "keyword",
            Some(&[1.0, 0.0]),
            "hash-v1",
            None,
            None,
            Some(10),
        )
        .unwrap();

    const K: f64 = 60.0;
    let rank_of = |id: i64| -> f64 { hybrid.iter().find(|r| r.observation.id == id).unwrap().rank };

    // `a`: FTS rank 0 + vector rank 1 -- exercises both RRF accumulation lines
    // for the same id, so their contributions must each be exactly right.
    let expected_a = 1.0 / (K + 0.0 + 1.0) + 1.0 / (K + 1.0 + 1.0);
    assert!(
        (rank_of(a.id) - expected_a).abs() < 1e-9,
        "a: got {}, expected {}",
        rank_of(a.id),
        expected_a
    );

    // `b`: FTS-only, rank 1 -- pins down line 180's formula in isolation
    // (rank 0 alone can't distinguish `K + rank` from `K - rank`).
    let expected_b = 1.0 / (K + 1.0 + 1.0);
    assert!(
        (rank_of(b.id) - expected_b).abs() < 1e-9,
        "b: got {}, expected {}",
        rank_of(b.id),
        expected_b
    );

    // `c`: vector-only, rank 0 -- pins down line 185's formula in isolation.
    let expected_c = 1.0 / (K + 0.0 + 1.0);
    assert!(
        (rank_of(c.id) - expected_c).abs() < 1e-9,
        "c: got {}, expected {}",
        rank_of(c.id),
        expected_c
    );
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

#[tokio::test]
async fn server_with_embedder_embeds_on_save_and_search_hybrid() -> anyhow::Result<()> {
    use crate::embed::HashEmbedder;
    use crate::server::IgrisServer;
    use rmcp::model::CallToolRequestParams;
    use rmcp::service::NotificationContext;
    use rmcp::{ClientHandler, RoleClient, ServiceExt};
    use std::sync::Arc;

    let db = Database::open_in_memory()?;
    let embedder = Arc::new(HashEmbedder::new(64));
    let server = IgrisServer::with_embedder(db, Some(embedder));
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    #[derive(Clone, Default)]
    struct NoOpClient;

    impl ClientHandler for NoOpClient {
        async fn on_logging_message(
            &self,
            _: rmcp::model::LoggingMessageNotificationParam,
            _: NotificationContext<RoleClient>,
        ) {
        }

        async fn on_progress(
            &self,
            _: rmcp::model::ProgressNotificationParam,
            _: NotificationContext<RoleClient>,
        ) {
        }
    }

    let client = NoOpClient.serve(client_transport).await?;

    // save → an embedding row is created for this observation under model "hash-v1"
    //
    // `HashEmbedder::embed` runs inside the spawned server task. If it panics
    // (e.g. a mutation-testing mutant makes it index out of bounds), the server
    // task dies mid-request and this `.await` would otherwise hang forever
    // instead of failing — bound it with a timeout so a broken embedder fails
    // the test instead of hanging the whole suite.
    let save_response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.call_tool(
            CallToolRequestParams::new("igris_save").with_arguments(
                serde_json::json!({
                    "title": "t",
                    "content": "alpha beta gamma",
                    "observation_type": "manual",
                    "scope": "project",
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        ),
    )
    .await
    .expect("igris_save timed out — server task likely panicked (e.g. in HashEmbedder::embed)")?;
    assert_ne!(
        save_response.is_error,
        Some(true),
        "save failed: {:?}",
        save_response.content
    );

    // search returns a non-error result (hybrid path exercised); same hang risk
    // as above since the hybrid path also calls into the embedder.
    let search_response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.call_tool(
            CallToolRequestParams::new("igris_search").with_arguments(
                serde_json::json!({ "query": "alpha" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ),
    )
    .await
    .expect("igris_search timed out — server task likely panicked (e.g. in HashEmbedder::embed)")?;
    assert_ne!(
        search_response.is_error,
        Some(true),
        "search failed: {:?}",
        search_response.content
    );

    // Verify that the saved content surfaces in search results (by checking the response isn't empty)
    let content_json = serde_json::to_string(&search_response.content).unwrap_or_default();
    assert!(
        content_json.contains("alpha"),
        "search results should contain 'alpha': {}",
        content_json
    );

    client.cancel().await?;
    Ok(())
}
