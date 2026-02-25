1. Remove Android effect tag whitelist logic from `AndroidEffectParser::parse` in `lazylog-android/src/parser.rs` so `-ae` no longer filters by Android logcat tag.
2. Keep Android effect behavior aligned with iOS effect mode by preserving only the structured marker check and `process_delta` parsing path.
3. Add or update Android parser unit tests in `lazylog-android/src/parser.rs` to verify structured logs with tags like `AE_JSRUNTIME_TAG` are accepted.
4. Keep or add a negative Android effect parser test to verify non-structured logs are still filtered out in `-ae`.
5. Run `cargo test -p lazylog-android` and report results with a brief manual verification note for `-ae`.
