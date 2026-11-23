use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, atomic::AtomicBool};

use rune::{TextEdit, Waker};

pub fn workspace_root() -> PathBuf {
    if let Some(root) = std::env::var_os("ZCSTREAM_ROOT") {
        let p = PathBuf::from(root);
        if p.exists() {
            return p;
        }
    }
    let has_repos = |p: &Path| {
        [
            "zcstream",
            "zcstream-main",
            "zcvid",
            "zcvid-main",
            "zcrudp",
            "zcrudp-main",
            "zpsync",
            "zpsync-main",
            "zcstream-app",
            "zcstream-app-main",
        ]
        .iter()
        .any(|d| p.join(d).is_dir())
    };
    let is_single_repo_root = |p: &Path| {
        (p.join("Cargo.toml").is_file() || p.join("src").join("stream_live.c").is_file())
            && !p.join("zcstream").is_dir()
    };
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors().skip(1) {
            if has_repos(anc) {
                return anc.to_path_buf();
            }
            if is_single_repo_root(anc)
                && let Some(parent) = anc.parent()
            {
                return parent.to_path_buf();
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for anc in cwd.ancestors() {
            if has_repos(anc) {
                return anc.to_path_buf();
            }
            if is_single_repo_root(anc)
                && let Some(parent) = anc.parent()
            {
                return parent.to_path_buf();
            }
        }
        return cwd;
    }
    PathBuf::from(".")
}

pub fn find_repo_dir_in(name: &str, root: &Path, exe_dir: Option<&Path>) -> PathBuf {
    for suffix in ["", "-main", "-master"] {
        let cand = root.join(format!("{name}{suffix}"));
        if cand.is_dir() {
            return cand;
        }
    }
    if let Some(ed) = exe_dir {
        for suffix in ["", "-main", "-master"] {
            let beside = ed.join(format!("{name}{suffix}"));
            if beside.is_dir() {
                return beside;
            }
        }
    }
    for zc_name in ["zcstream", "zcstream-main", "zcstream-master"] {
        let fetched = root
            .join(zc_name)
            .join("build")
            .join("_deps")
            .join(format!("{name}-src"));
        if fetched.is_dir() {
            return fetched;
        }
        let nested = root.join(zc_name).join("deps").join(name);
        if nested.is_dir() {
            return nested;
        }
    }
    root.join(name)
}

pub fn find_repo_dir(name: &str) -> PathBuf {
    let root = workspace_root();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    find_repo_dir_in(name, &root, exe_dir.as_deref())
}

pub fn zcstream_dir() -> PathBuf {
    find_repo_dir("zcstream")
}

pub fn zcvid_dir() -> PathBuf {
    find_repo_dir("zcvid")
}

pub fn zcrudp_dir() -> PathBuf {
    find_repo_dir("zcrudp")
}

pub fn zpsync_dir() -> PathBuf {
    find_repo_dir("zpsync")
}

pub fn bin_name(base: &str) -> String {
    format!("{base}{}", std::env::consts::EXE_SUFFIX)
}

pub fn find_binary_in(
    base: &str,
    env_bin_dir: Option<&Path>,
    exe_dir: Option<&Path>,
    root: &Path,
    path_env: Option<&OsStr>,
) -> Option<PathBuf> {
    let name = bin_name(base);
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(dir) = env_bin_dir {
        dirs.push(dir.to_path_buf());
    }
    if let Some(ed) = exe_dir {
        dirs.push(ed.to_path_buf());
        dirs.push(ed.join("bin"));
    }

    dirs.push(root.join("dist"));

    for build_base in [
        root.join("zcstream").join("build"),
        root.join("zcstream-main").join("build"),
        root.join("zpsync").join("build"),
        root.join("zpsync-main").join("build"),
        root.join("zcstream-app").join("zcstream").join("build"),
    ] {
        dirs.push(build_base.clone());
        dirs.push(build_base.join("Release"));
        dirs.push(build_base.join("RelWithDebInfo"));
        dirs.push(build_base.join("Debug"));
    }

    for dir in dirs {
        let candidate = dir.join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let candidate = dir.join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

pub fn find_binary(base: &str) -> Option<PathBuf> {
    let env_bin_dir = std::env::var_os("ZCSTREAM_BIN_DIR").map(PathBuf::from);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let root = workspace_root();
    let path_env = std::env::var_os("PATH");
    find_binary_in(
        base,
        env_bin_dir.as_deref(),
        exe_dir.as_deref(),
        &root,
        path_env.as_deref(),
    )
}

pub fn host_os_label() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "macOS",
        other => other,
    }
}

pub fn host_arch_label() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "ARM64",
        other => other,
    }
}

pub fn host_platform_badge() -> String {
    format!("{} {}", host_os_label(), host_arch_label())
}

pub fn host_supports_nvenc() -> bool {
    cfg!(all(target_os = "windows", target_arch = "x86_64"))
}

pub fn host_supports_rio() -> bool {
    cfg!(target_os = "windows")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Benchmarks,
    Tests,
    Network,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineKind {
    Nvenc,
    Pyrowave,
    Zcvid,
}

impl PipelineKind {
    pub fn label(self) -> &'static str {
        match self {
            PipelineKind::Nvenc => "NVENC Direct (H.264)",
            PipelineKind::Pyrowave => "Pyrowave (Wavelets)",
            PipelineKind::Zcvid => {
                if cfg!(target_os = "windows") {
                    "Zcvid Unified (GPU)"
                } else {
                    "Zcvid Unified (Portable)"
                }
            }
        }
    }

    pub fn base_exe_name(self) -> &'static str {
        match self {
            PipelineKind::Nvenc => "zcrudp_stream_bench",
            PipelineKind::Pyrowave => "zcrudp_pyrowave_bench",
            PipelineKind::Zcvid => "zcrudp_zcvid_bench",
        }
    }

    pub fn exe_name(self) -> String {
        bin_name(self.base_exe_name())
    }

    pub fn is_supported_on_platform(self) -> bool {
        match self {
            PipelineKind::Nvenc => host_supports_nvenc(),
            PipelineKind::Pyrowave | PipelineKind::Zcvid => true,
        }
    }

    pub fn is_binary_present(self) -> bool {
        find_binary(self.base_exe_name()).is_some()
    }

    pub fn summary(self) -> &'static str {
        match self {
            PipelineKind::Nvenc => {
                if host_supports_nvenc() {
                    "nvEncodeAPI64 + D3D11/DXGI Zero-Copy (~3.3 ms p50)"
                } else {
                    "Indisponible hors Windows x64 (utilisez Zcvid Unified ou Pyrowave)"
                }
            }
            PipelineKind::Pyrowave => {
                if host_supports_rio() {
                    "Vulkan Compute Wavelets + Winsock RIO (~1.3 ms p50)"
                } else {
                    "Vulkan Compute Wavelets + UDP POSIX"
                }
            }
            PipelineKind::Zcvid => {
                if host_supports_rio() {
                    "Codec unique HLSL 4x4/8x8 Transform RDO + QPel MV + CABAC + Winsock RIO"
                } else {
                    "Codec unique portable C11 4x4/8x8 Transform RDO + QPel MV + CABAC + UDP POSIX"
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionPreset {
    pub label: &'static str,
    pub width: u32,
    pub height: u32,
}

pub const RESOLUTIONS: &[ResolutionPreset] = &[
    ResolutionPreset { label: "1280×720", width: 1280, height: 720 },
    ResolutionPreset { label: "1920×1080", width: 1920, height: 1080 },
    ResolutionPreset { label: "2560×1440", width: 2560, height: 1440 },
    ResolutionPreset { label: "3840×2160", width: 3840, height: 2160 },
];

#[derive(Clone, Debug)]
pub struct NetImpair {
    pub nasty_preset: bool,
    pub loss_pct: i32,
    pub burst_prob_pct: i32,
    pub burst_len: i32,
    pub reorder_pct: i32,
    pub dup_pct: i32,
    pub base_owd_us: u32,
    pub jitter_us: u32,
}

impl Default for NetImpair {
    fn default() -> Self {
        Self {
            nasty_preset: false,
            loss_pct: 0,
            burst_prob_pct: 0,
            burst_len: 3,
            reorder_pct: 0,
            dup_pct: 0,
            base_owd_us: 0,
            jitter_us: 0,
        }
    }
}

impl NetImpair {
    pub fn apply_nasty(&mut self) {
        self.nasty_preset = true;
        self.loss_pct = 5;
        self.burst_prob_pct = 8;
        self.burst_len = 3;
        self.reorder_pct = 15;
        self.dup_pct = 5;
        self.base_owd_us = 2000;
        self.jitter_us = 3000;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Debug)]
pub struct StatRow {
    pub label: String,
    pub min_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub avg_us: f64,
}

#[derive(Clone, Debug, Default)]
pub struct BenchResults {
    pub adapter: String,
    pub encoder: String,
    pub frames_sent: Option<u64>,
    pub frames_delivered: Option<u64>,
    pub delivered_pct: Option<f64>,
    pub fec_recovered: Option<u64>,
    pub frames_dropped: Option<u64>,
    pub pkts_sent: Option<u64>,
    pub pkts_dropped_sim: Option<u64>,
    pub pkts_reordered: Option<u64>,
    pub pkts_dup: Option<u64>,
    pub pkts_fec: Option<u64>,
    pub send_errors: Option<u64>,
    pub avg_mbps: Option<f64>,
    pub kb_per_frame: Option<f64>,
    pub y_psnr_db: Option<f64>,
    pub ref_mismatch_px: Option<u64>,
    pub rows: Vec<StatRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestStatus {
    Idle,
    Running,
    Passed,
    Failed,
}

impl TestStatus {
    pub fn label(self) -> &'static str {
        match self {
            TestStatus::Idle => "Prêt",
            TestStatus::Running => "En cours...",
            TestStatus::Passed => "OK",
            TestStatus::Failed => "Échec",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestTargetId {
    StreamingBridge,
    ZcvidCodec,
    ZcrudpCore,
    Zpsync,
}

#[derive(Clone, Debug)]
pub struct TestEntry {
    pub id: TestTargetId,
    pub name: &'static str,
    pub subtitle: &'static str,
    pub status: TestStatus,
    pub duration_ms: Option<u64>,
    pub logs: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRole {
    Sender,
    Receiver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetConnectMode {
    DirectUdp,
    ZpSync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamCodec {
    Nvenc,
    Pyrowave,
    Zcvid,
}

impl StreamCodec {
    pub fn as_arg(self) -> &'static str {
        match self {
            StreamCodec::Nvenc => "nvenc",
            StreamCodec::Pyrowave => "pyrowave",
            StreamCodec::Zcvid => "zcvid",
        }
    }

    pub fn is_supported_on_platform(self) -> bool {
        match self {
            StreamCodec::Nvenc => host_supports_nvenc(),
            StreamCodec::Pyrowave | StreamCodec::Zcvid => true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NetSessionStats {
    pub running: bool,
    pub fps: f64,
    pub mbps: f64,
    pub frames_tx: u64,
    pub frames_rx: u64,
    pub fec_recovered: u64,
    pub frames_dropped: u64,
    pub packets_total: u64,
    pub status_msg: String,
    pub zp_path: String,
    pub zp_session: String,
    pub zp_rtt_ms: Option<u32>,
}

pub enum AppEvent {
    BenchLog(String),
    BenchDone { exit_code: i32 },
    TestLog { id: TestTargetId, line: String },
    TestDone { id: TestTargetId, passed: bool, duration_ms: u64 },
    NetStats(NetSessionStats),
    NetLog(String),
    NetStopped(String),
    ZpProbeDone { summary: String, logs: Vec<String> },
}

pub struct AppState {
    pub dark: bool,
    pub tab: Tab,
    pub waker: Option<Waker>,
    pub tx: Sender<AppEvent>,
    pub rx: Receiver<AppEvent>,

    // Benchmarks tab
    pub pipeline: PipelineKind,
    pub res_idx: usize,
    pub bench_fps: u32,
    pub bench_fps_edit: TextEdit,
    pub frames: usize,
    pub bitrate_mbps: u32,
    pub bench_bitrate_edit: TextEdit,
    pub fec_shards: u8,
    pub use_rio: bool,
    pub live_window: bool,
    pub measure_psnr: bool,
    pub impair: NetImpair,
    pub bench_running: bool,
    pub bench_exit_code: Option<i32>,
    pub bench_stop: Option<Arc<AtomicBool>>,
    pub bench_results: BenchResults,
    pub bench_logs: Vec<String>,
    pub show_raw_logs: bool,

    // Tests tab
    pub tests: Vec<TestEntry>,
    pub selected_test: usize,

    // Network streaming tab
    pub net_role: NetRole,
    pub net_connect_mode: NetConnectMode,
    pub net_codec: StreamCodec,
    pub net_res_idx: Option<usize>,
    pub net_width_edit: TextEdit,
    pub net_height_edit: TextEdit,
    pub net_host_edit: TextEdit,
    pub net_port_edit: TextEdit,
    pub zp_secret_edit: TextEdit,
    pub zp_peer_ip_edit: TextEdit,
    pub zp_peer_port_edit: TextEdit,
    pub zp_local_port_edit: TextEdit,
    pub zp_birthday: bool,
    pub zp_sentinel: bool,
    pub zp_probing: bool,
    pub zp_probe_info: Option<String>,
    pub net_fps: u32,
    pub net_fps_edit: TextEdit,
    pub net_bitrate_mbps: u32,
    pub net_bitrate_edit: TextEdit,
    pub net_fec_shards: u8,
    pub net_use_rio: bool,
    pub net_stop: Option<Arc<AtomicBool>>,
    pub net_stats: NetSessionStats,
    pub net_logs: Vec<String>,
}

impl Default for AppState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let net_host_edit = TextEdit::new("127.0.0.1");
        let net_port_edit = TextEdit::new("9900");
        let net_width_edit = TextEdit::new("1920");
        let net_height_edit = TextEdit::new("1080");
        let bench_fps_edit = TextEdit::new("120");
        let bench_bitrate_edit = TextEdit::new("");
        let net_fps_edit = TextEdit::new("120");
        let net_bitrate_edit = TextEdit::new("");
        let zp_secret_edit = TextEdit::new("zcstream-p2p");
        let zp_peer_ip_edit = TextEdit::new("");
        let zp_peer_port_edit = TextEdit::new("0");
        let zp_local_port_edit = TextEdit::new("0");

        Self {
            dark: true,
            tab: Tab::Benchmarks,
            waker: None,
            tx,
            rx,
            pipeline: PipelineKind::Zcvid,
            res_idx: 1, // 1920x1080
            bench_fps: 120,
            bench_fps_edit,
            frames: 240,
            bitrate_mbps: 50,
            bench_bitrate_edit,
            fec_shards: 1,
            use_rio: host_supports_rio(),
            live_window: false,
            measure_psnr: true,
            impair: NetImpair::default(),
            bench_running: false,
            bench_exit_code: None,
            bench_stop: None,
            bench_results: BenchResults::default(),
            bench_logs: Vec::new(),
            show_raw_logs: false,
            tests: vec![
                TestEntry {
                    id: TestTargetId::StreamingBridge,
                    name: "zcstream / portable + bridge",
                    subtitle: "ctest --test-dir zcstream/build (fragmentation FEC + codec portable C11)",
                    status: TestStatus::Idle,
                    duration_ms: None,
                    logs: Vec::new(),
                },
                TestEntry {
                    id: TestTargetId::ZcvidCodec,
                    name: "zcvid / zcvid_codec",
                    subtitle: "make test (trit packing + codec SIMD/scalar + CPU GPU-dec + 18 preuves)",
                    status: TestStatus::Idle,
                    duration_ms: None,
                    logs: Vec::new(),
                },
                TestEntry {
                    id: TestTargetId::ZcrudpCore,
                    name: "zcrudp / core",
                    subtitle: "ctest --test-dir zcrudp/build (session, reliability, crypto, nextgen)",
                    status: TestStatus::Idle,
                    duration_ms: None,
                    logs: Vec::new(),
                },
                TestEntry {
                    id: TestTargetId::Zpsync,
                    name: "zpsync / nat_traversal",
                    subtitle: "ctest --test-dir zpsync/build (crypto, DHT, STUN, ICMP sentinel, punch)",
                    status: TestStatus::Idle,
                    duration_ms: None,
                    logs: Vec::new(),
                },
            ],
            selected_test: 0,
            net_role: NetRole::Sender,
            net_connect_mode: NetConnectMode::DirectUdp,
            net_codec: StreamCodec::Zcvid,
            net_res_idx: Some(1),
            net_width_edit,
            net_height_edit,
            net_host_edit,
            net_port_edit,
            zp_secret_edit,
            zp_peer_ip_edit,
            zp_peer_port_edit,
            zp_local_port_edit,
            zp_birthday: false,
            zp_sentinel: true,
            zp_probing: false,
            zp_probe_info: None,
            net_fps: 120,
            net_fps_edit,
            net_bitrate_mbps: 50,
            net_bitrate_edit,
            net_fec_shards: 1,
            net_use_rio: host_supports_rio(),
            net_stop: None,
            net_stats: NetSessionStats {
                status_msg: format!(
                    "Prêt à lancer {} ({}).",
                    bin_name("zcrudp_stream"),
                    host_platform_badge()
                ),
                ..Default::default()
            },
            net_logs: Vec::new(),
        }
    }
}

impl AppState {
    /// Recommended auto bandwidth (Mbps) from pixel rate (matches zcrudp_stream).
    pub fn auto_bitrate_mbps(width: u32, height: u32, fps: u32) -> u32 {
        let px_per_sec = width as u64 * height as u64 * fps.max(1) as u64;
        let auto = 20 + px_per_sec / 20_000_000;
        (auto as u32).clamp(25, 200)
    }

    /// Minimum bandwidth (Mbps) required to sustain a given resolution and refresh rate cleanly.
    pub fn min_mbps_for_mode(width: u32, height: u32, fps: u32) -> u32 {
        let px_per_sec = width as u64 * height as u64 * fps.max(1) as u64;
        let mbps = 3 + px_per_sec.div_ceil(25_000_000);
        (mbps as u32).max(4)
    }

    pub fn effective_bench_fps(&self) -> u32 {
        self.bench_fps_edit
            .text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|&v| v > 0)
            .unwrap_or(self.bench_fps)
            .clamp(1, 1000)
    }

    pub fn is_bench_bitrate_auto(&self) -> bool {
        let raw = self.bench_bitrate_edit.text.trim();
        raw.is_empty() || raw == "0"
    }

    pub fn effective_bench_bitrate_mbps(&self) -> u32 {
        let res = RESOLUTIONS[self.res_idx];
        let fps = self.effective_bench_fps();
        if self.is_bench_bitrate_auto() {
            return Self::auto_bitrate_mbps(res.width, res.height, fps);
        }
        self.bench_bitrate_edit
            .text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|&v| v > 0)
            .unwrap_or(self.bitrate_mbps)
            .clamp(1, 1000)
    }

    pub fn effective_net_dims(&self) -> (u32, u32) {
        let w = self.net_width_edit.text.trim().parse::<u32>().unwrap_or(1920).max(64);
        let h = self.net_height_edit.text.trim().parse::<u32>().unwrap_or(1080).max(64);
        (w, h)
    }

    pub fn effective_net_fps(&self) -> u32 {
        self.net_fps_edit
            .text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|&v| v > 0)
            .unwrap_or(self.net_fps)
            .clamp(1, 1000)
    }

    pub fn is_net_bitrate_auto(&self) -> bool {
        let raw = self.net_bitrate_edit.text.trim();
        raw.is_empty() || raw == "0"
    }

    pub fn effective_net_bitrate_mbps(&self) -> u32 {
        if self.is_net_bitrate_auto() {
            return 0; // auto (computed by zcrudp_stream or auto_bitrate_mbps)
        }
        self.net_bitrate_edit
            .text
            .trim()
            .parse::<u32>()
            .unwrap_or(self.net_bitrate_mbps)
            .min(1000)
    }

    pub fn build_bench_args(&self) -> Vec<String> {
        let res = RESOLUTIONS[self.res_idx];
        let fps = self.effective_bench_fps();
        let bitrate_mbps = self.effective_bench_bitrate_mbps();
        let mut args = vec![
            "--width".into(),
            res.width.to_string(),
            "--height".into(),
            res.height.to_string(),
            "--frames".into(),
            self.frames.to_string(),
        ];

        match self.pipeline {
            PipelineKind::Nvenc => {
                args.push("--fps".into());
                args.push(fps.to_string());
                args.push("--bitrate".into());
                args.push((bitrate_mbps as u64 * 1_000_000).to_string());
                if self.live_window {
                    args.push("--live".into());
                }
            }
            PipelineKind::Pyrowave => {
                // target-bytes per frame: bitrate_bps / (8 * fps)
                let target_bytes = (bitrate_mbps as u64 * 1_000_000) / (8 * fps.max(1) as u64);
                args.push("--fps".into());
                args.push(fps.to_string());
                args.push("--target-bytes".into());
                args.push(target_bytes.max(1000).to_string());
                if host_supports_rio() && !self.use_rio {
                    args.push("--no-rio".into());
                }
            }
            PipelineKind::Zcvid => {
                args.push("--fps".into());
                args.push(fps.to_string());
                args.push("--bitrate".into());
                args.push((bitrate_mbps as u64 * 1_000_000).to_string());
                if host_supports_rio() && !self.use_rio {
                    args.push("--no-rio".into());
                }
                if self.measure_psnr {
                    args.push("--psnr".into());
                }
                if self.live_window {
                    args.push("--live".into());
                }
            }
        }

        if self.fec_shards == 0 {
            args.push("--no-fec".into());
        } else {
            args.push("--fec".into());
            args.push(self.fec_shards.to_string());
        }

        if self.impair.nasty_preset {
            args.push("--nasty".into());
        } else {
            if self.impair.loss_pct > 0 {
                args.push("--loss".into());
                args.push(self.impair.loss_pct.to_string());
            }
            if self.impair.burst_prob_pct > 0 {
                args.push("--burst".into());
                args.push(self.impair.burst_prob_pct.to_string());
                args.push(self.impair.burst_len.to_string());
            }
            if self.impair.reorder_pct > 0 {
                args.push("--reorder".into());
                args.push(self.impair.reorder_pct.to_string());
            }
            if self.impair.dup_pct > 0 {
                args.push("--dup".into());
                args.push(self.impair.dup_pct.to_string());
            }
            if self.impair.base_owd_us > 0 {
                args.push("--owd".into());
                args.push(self.impair.base_owd_us.to_string());
            }
            if self.impair.jitter_us > 0 {
                args.push("--jitter".into());
                args.push(self.impair.jitter_us.to_string());
            }
        }

        args
    }

    pub fn cli_preview(&self) -> String {
        let exe = self.pipeline.exe_name();
        let args = self.build_bench_args().join(" ");
        format!("{exe} {args}")
    }
}
