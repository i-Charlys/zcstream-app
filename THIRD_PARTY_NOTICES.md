# Third-Party Notices and Citations (`zcstream-app` & RUDP Suite)

`zcstream-app` is licensed under the **MIT License** (see [LICENSE](LICENSE)).

## Suite Repositories (All MIT License, Copyright (c) 2026 CAILLON Charles)

- **`zcstream-app`** (`https://github.com/i-Charlys/zcstream-app`) — Cross-platform streaming GUI (`Rune` + `wgpu`).
- **`zcstream`** (`https://github.com/i-Charlys/zcstream`) — Streaming engine, GPU/portable capture, packet framing, and `zcrudp_stream`.
- **`zcvid`** (`https://github.com/i-Charlys/zcvid`) — Unified hybrid transform + adaptive ternary delta video codec (`HLSL` & portable `C11`).
- **`zcrudp`** (`https://github.com/i-Charlys/zcrudp`) — Reliable low-latency UDP transport, SCReAM/L4S pacing, and ChaCha20-Poly1305 AEAD.
- **`zpsync`** (`https://github.com/i-Charlys/zpsync`) — Peer-to-peer NAT traversal (STUN, Mainline DHT, X25519 PFS, Birthday Paradox, ICMP Sentinel).

---

## Third-Party Libraries & Crates

### GUI & Graphics (`zcstream-app`)
- **Rune / `rune-kit` / `rune-theme-opencode`** (including patched `wgpu-hal`, `winit`, `parley_engine`, `accesskit_windows`, `accesskit_macos`): https://github.com/anomalyco/rune (MIT / Apache-2.0).
- **`wgpu` / `wgpu-core` / `naga`**: gfx-rs community, MIT / Apache-2.0.
- **`winit`**: Rust Windowing contributors, Apache-2.0.
- **`taffy`**, **`parley`**, **`skrifa`**, **`harfrust`**, **`resvg`**, **`usvg`**, **`tiny-skia`**: Permissive open-source licenses (`MIT` / `Apache-2.0` / `Zlib` / `BSD-3-Clause`).
- **`serde`**, **`serde_json`**, **`blake3`**, **`arboard`**, **`clipboard-win`**, **`tracing`**, **`windows`**: Permissive open-source licenses (`MIT` / `Apache-2.0` / `CC0-1.0`).

### Video Codec Headers & Backends (`zcstream`)
- **FFmpeg `nv-codec-headers` (`nvEncodeAPI.h`)**: https://github.com/FFmpeg/nv-codec-headers — MIT License.
- **Pyrowave (optional Vulkan Wavelet backend)**: Hans-Kristian Arntzen (Themaister), https://github.com/Themaister/pyrowave — MIT License.

---

## Standards & Academic Citations

- **Authenticated Encryption**: Y. Nir, A. Langley, *"ChaCha20 and Poly1305 for IETF Protocols"*, IETF RFC 8439, June 2018.
- **Key Exchange**: A. Langley, M. Hamburg, S. Turner, *"Elliptic Curves for Security (X25519)"*, IETF RFC 7748; D. J. Bernstein, *"Curve25519: New Diffie-Hellman Speed Records"*, PKC 2006.
- **Congestion Control & Reliability**: IETF RFC 8298 (*SCReAM*), IETF RFC 6582 (*NewReno*), IETF RFC 2018 (*SACK*).
- **Erasure Coding (Cauchy GF(2^8) MDS FEC)**: J. Bloemer, M. Kalfane, R. Karp, M. Karpinski, M. Luby, D. Zuckerman, *"An XOR-Based Erasure-Resilient Coding Scheme"*, ICSI TR-95-048, 1995.
- **NAT Traversal**: IETF RFC 5389 / RFC 8489 (*STUN*), BitTorrent BEP-0005 (*Mainline DHT*), IETF RFC 1071 (*Internet Checksum*), NIST FIPS 180-4, IETF RFC 2104.
- **Video Transform & Quantization (`zcvid`)**:
  - H. S. Malvar, A. Hallapuro, M. Karczewicz, L. Kerofsky, *"Low-Complexity Transform and Quantization in H.264/AVC"*, IEEE TCSVT, July 2003 (ITU-T H.264).
  - ITU-T H.265 / HEVC (4-tap quarter-pel motion compensation filter).
  - N. S. Jayant, *"Adaptive Quantization With a One-Word Memory"*, BSTJ, September 1973.
  - G. N. N. Martin (1979) / A. Moffat, R. M. Neal, I. H. Witten (1998), *"Arithmetic Coding Revisited"* (Range coding).
