# net tests (28 files, 9251 lines) — agent 697c60fe
P1 browser_features_tests.rs:670-691 — test blesses s-maxage in a private cache (RFC 9111 5.2.2.1): asserts some(Duration::from_secs(90)) for `s-maxage=120, Age: 30`; crates/net/src/cache.rs:79-83 returns Some(s_maxage). Fix: ignore s_maxage in explicit_lifetime.
P1 browser_features_tests.rs:694-712 — must-revalidate revalidates FRESH entries; cache.rs:98-101 `pub const fn requires_revalidation(&self) -> bool { self.no_cache || self.must_revalidate }` has no staleness input; test asserts the wrong side at :711.
P1 upload_streaming.rs:354-362 — `std::thread::spawn(move || { ... handler(&mut stream); });` handle dropped; tests :432/:472/:496/:529 never join so asserts at :436/:446/:481 are dead.
P1 h2_advanced.rs:73-81 — spawn_server drops handle; :213-216 assert_eq!(order, vec!["/one","/high","/low"]) and :407 enable_push==Some(1) run in that thread; :239-253 accepts any Err(Http2).
P1 request_bodies.rs:71-103 — collect_request_body hardcodes END_STREAM=true and empty trailers → :259 assert!(end_seen) tautology, request trailers never sent.
P2 trailers_tests.rs:206-249 — helper named ..._with_end_stream never reads end_stream, so :252-282 passes if trailers omit END_STREAM.
P2 trailers_tests.rs:160-183 — bad request trailer surfaces as FetchError::InvalidResponse (crates/net/src/http.rs:1155-1162), test locks it in at :172; should be FetchError::RequestBody.
P2 streaming_pool_tests.rs:284-323 — `let _ = server.join();` discards the only RST_STREAM/CANCEL assertion (:299-302).
P2 h2_flow_multiplex_tests.rs:478-518 — MAX_CONCURRENT_STREAMS checked by a 150 ms negative window (also 150 ms sleep at :351).
P2 push_priority_tests.rs:101-119 — RST_STREAM on a 400 ms window (:164/:212/:247) and `let _ = code;` at :108-112 discards the code.
P2 streaming_compression_tests.rs:110-135 — incrementality forced by sleeps 20-40 ms (:117,:120,:160,:203), asserted as calls>=3 (:135,:174).
P3 cache_tests.rs:104-117 — hard-coded `Expires: Tue, 01 Jan 2030 00:00:00 GMT` time bomb.
P3 advanced_tests.rs:444-642 — CORS negatives matched by Display substring (contains("Allow-Origin") :450, ("not allowed") :469, ("credentials") :492, ("DELETE") :609, ("origin") :642).
Additional verified, cut for size: h1_streaming_tests.rs:78-122 (sleeps + calls>=3, same as compression); url_userinfo_auth.rs:64-118 (head.to_ascii_lowercase() masks Base64 case errors); push_loader_tests.rs:221-224 (pushed_discarded()<=1 always true); proxy_pool_tests.rs:1 vs 94-125 (cap 4 / idle timeout documented but untested); data_tests.rs:21 (is_err() without type); http_tests.rs:284,306,321,333,361,377 and request_bodies.rs:122 (`let _ = join`); spawn_server duplicated literally in advanced_tests.rs:28-59, http_tests.rs:25-56, browser_features_tests.rs:32-63. No findings in url_tests.rs, mime_tests.rs (high quality).
