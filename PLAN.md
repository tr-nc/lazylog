1. Add separate timing controls for input and provider polling in `lazylog-framework/src/app/mod.rs`.
2. Update the app loop to poll keyboard and mouse events at high frequency (target about 16ms) while keeping provider ingestion on its own interval.
3. Add an interruptible sleep helper in `lazylog-framework/src/provider/mod.rs` that checks stop state in short slices (target 20 to 50ms).
4. Replace provider thread blocking sleep with the interruptible sleep helper so shutdown does not wait for full poll interval.
5. Add immediate stop signaling on quit keys in `lazylog-framework/src/app/events.rs` so `q` and `Ctrl-c` trigger provider interruption right away.
6. Add interruptible retry and backoff waits in `lazylog-android/src/provider.rs` so no device reconnect loops can delay exit.
7. Add interruptible retry and backoff waits in `lazylog-ios/src/provider.rs` so no device reconnect loops can delay exit.
8. Keep behavior consistent across providers by using the same stop check cadence and helper pattern where possible.
9. Verify key responsiveness remains snappy during normal operation and under no-device scenarios for Android and iOS modes.
10. Measure and record before and after quit latency with no device connected, and ensure exit is consistently sub 100ms to 150ms.
11. Update docs in `README.md` and `lazylog-framework/README.md` to describe responsive quit behavior and architecture change.
12. Run project checks and tests, then fix any regressions.
