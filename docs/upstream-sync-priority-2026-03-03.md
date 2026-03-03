# Upstream Sync Priority (2026-03-03)

## Scope

- Compare range: `515f0b4..9bddb3e` on `upstream/main`
- Time window: 2026-03-02 (upstream tags `v1.12.3` ~ `v1.13.1`)
- Goal: define **landable, necessary** sync priorities for this Rust fork (`rliamp`)

## Priority Matrix

### P0 (Must sync first, release-impacting)

1. [x] Security and robustness hardening (`f2411eb`) — completed in `4dc8f45`
- Why: includes multiple bug/security class fixes (path traversal, provider error handling, process/resource issues).
- Land in this repo:
  - `src/navidrome.rs`: strengthen Subsonic/Navidrome response error checks and non-200 handling.
  - `src/main.rs`: tighten playlist path/url resolution boundaries for local/remote input.
  - `src/player.rs`: audit ffmpeg subprocess lifecycle and ensure no orphan/leak in decode fallback paths.
- Acceptance:
  - malformed provider responses fail gracefully with explicit error message
  - no residual ffmpeg child process after repeated stream start/stop cycles
  - path traversal vectors in playlist entries are rejected or safely normalized

2. [~] Stream compatibility for radio/AAC+ (`e254668`) — partial landing in `f8a4ec1`
- Why: directly affects “can play / cannot play” for real-world stations.
- Land in this repo:
  - `src/player.rs`: add/adjust streaming ffmpeg decode path for AAC+ cases.
  - `src/main.rs` + resolver functions: improve format detection for stream sources.
- Acceptance:
  - known AAC+ station URLs that previously failed can play continuously
  - no early EOF regressions for common radio streams

3. [x] Content sniff for feed/playlist detection (`fbd6ade`)
- Why: many feed URLs do not end with `.xml/.m3u/.pls`; extension-only detection misses valid sources.
- Land in this repo:
  - `src/main.rs`: when URL suffix is ambiguous, inspect `Content-Type` and/or body prefix before resolver routing.
- Acceptance:
  - URL without `.xml` suffix can still be identified as podcast feed when response indicates XML
  - backward compatibility for existing extension-based URLs

### P1 (High value, should follow P0)

1. [x] Visualizer-off resource optimization (`7e50a54`, `ddd6cb5`) — completed in `8592f3b`
- Why: measurable CPU/GPU savings on laptops and remote terminals.
- Land in this repo:
  - `src/visualizer.rs`: add `VisNone`.
  - `src/ui.rs`: add toggle key and skip FFT/render when visualizer disabled; adaptive tick interval.
- Acceptance:
  - CPU usage drops significantly when visualizer is off
  - behavior is stable when toggling on/off repeatedly

2. [x] Navidrome config-section + browser improvements (`fab9eb7`, `74e562f`, `0daddce`) — completed in `54ba028`
- Why: env-only setup is less manageable; upstream moved to config section and expanded browser flow.
- Land in this repo:
  - `src/config.rs`: add `[navidrome]` section parsing (URL/user/pass/token).
  - `src/navidrome.rs`, `src/ui.rs`: align provider browse/load UX and fallback logic.
- Acceptance:
  - provider works from config file without env vars
  - empty/error provider states are recoverable in UI

3. [x] Legacy metadata decoding robustness (`ff71b42`) — completed in `5280618`
- Why: improves display correctness for non-Latin tags.
- Land in this repo:
  - `src/playlist.rs` (and metadata extraction paths): add charset fallback decode.
- Acceptance:
  - representative legacy-encoded files display title/artist correctly (no mojibake)

### P2 (Optional / UX polish)

1. [x] 80s synthwave visualizer (`9bddb3e`) — completed in `12c7ef3`
- Visual polish feature; not playback-critical.

2. [ ] UI redesign batch (`b6987e4`, parts of `ed8b58c`)
- Mostly presentation adjustments; can defer.

3. [ ] Site/homebrew workflow/document formatting commits
- Mostly upstream website/release pipeline concerns; not core runtime behavior.

## Suggested Execution Order

1. P0-1 hardening baseline
2. P0-2 AAC+ stream compatibility
3. P0-3 content sniff routing
4. P1-1 visualizer off + adaptive tick
5. P1-2 Navidrome config/browser
6. P1-3 metadata decode fallback
7. P2 polish items

## Notes for This Fork

- This fork already diverged with custom keymap/theme/overlay behavior; sync should prefer **behavioral parity** over literal upstream UI structure.
- For large upstream commits, split into small Rust-native commits by concern (decode, resolver, provider, visualizer), each with reproducible tests/manual checks.
