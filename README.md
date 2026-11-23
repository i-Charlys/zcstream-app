# zcstream-app — RUDP Control Center

Native GPU-accelerated desktop GUI (`rune` + `wgpu`) for benchmarking, testing, and live P2P streaming across the RUDP video stack (`zcstream`, `zcvid`, `zcrudp`, `zpsync`).

## One-Command Installation (Windows, Linux, macOS)

You do **not** need to manually clone the 5 repositories or edit any file paths. Clone `zcstream-app` and run the included installer script:

### Windows (PowerShell)
```powershell
git clone https://github.com/i-Charlys/zcstream-app.git
cd zcstream-app
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

### Linux / macOS
```bash
git clone https://github.com/i-Charlys/zcstream-app.git
cd zcstream-app
chmod +x ./install.sh
./install.sh
```

### What the installer does automatically
1. Clones (or updates) sibling repositories `zcrudp`, `zcvid`, `zpsync`, `zcstream`, and `zcstream-app` in the parent workspace folder.
2. Auto-detects your C/C++ compiler (`clang`, `gcc`, or MSVC) and CMake generator.
3. Builds and tests `zcrudp`, `zcvid`, `zpsync`, and `zcstream` (auto-fetching `nvEncodeAPI.h` on Windows).
4. Builds `zcstream-app` with Cargo and stages a self-contained portable bundle (`zcstream-app`, `zcrudp_stream`, `zcrudp_zcvid_bench`, `zcrudp_stream_bench`, `zpsync_peer`, and `shaders/`) into `../dist/`.

## Environment Overrides (Optional)

`zcstream-app` automatically discovers binaries and repositories in:
1. Next to the `zcstream-app` executable (portable `dist/` bundle)
2. `../dist/` and `../zcstream/build/{,Release,Debug}` (workspace layout)
3. CMake `_deps/` directories (`../zcstream/build/_deps/<repo>-src`)
4. System `PATH`

You can also override paths via environment variables if desired:
- `ZCSTREAM_BIN_DIR`: directory containing `zcrudp_stream`, `zcrudp_zcvid_bench`, `zpsync_peer`, etc.
- `ZCSTREAM_ROOT`: root directory containing the sibling repositories.
