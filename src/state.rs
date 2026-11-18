use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, atomic::AtomicBool};

use rune::{TextEdit, Waker};

pub const ZCSTREAM_BUILD_DIR: &str = r"C:\Users\charl\Documents\RUDP\zcstream\build";
pub const ZCVID_DIR: &str = r"C:\Users\charl\Documents\RUDP\zcvid";
pub const ZCRUDP_DIR: &str = r"C:\Users\charl\Documents\RUDP\zcrudp";
pub const ZPSYNC_DIR: &str = r"C:\Users\charl\Documents\RUDP\zpsync";
pub const ZPSYNC_BUILD_DIR: &str = r"C:\Users\charl\Documents\RUDP\zpsync\build";

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
    Ternary,
}

impl PipelineKind {
    pub fn label(self) -> &'static str {
        match self {
            PipelineKind::Nvenc => "NVENC Direct (H.264)",
            PipelineKind::Pyrowave => "Pyrowave (Wavelets)",
            PipelineKind::Ternary => "Ternary HLSL (VRAM)",
        }
    }

    pub fn exe_name(self) -> &'static str {
        match self {
            PipelineKind::Nvenc => "zcrudp_stream_bench.exe",
            PipelineKind::Pyrowave => "zcrudp_pyrowave_bench.exe",
            PipelineKind::Ternary => "zcrudp_ternary_bench.exe",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            PipelineKind::Nvenc => "nvEncodeAPI64 + D3D11/DXGI Zero-Copy (~3.3 ms p50)",
            PipelineKind::Pyrowave => "Vulkan Compute Wavelets + Winsock RIO (~1.3 ms p50)",
            PipelineKind::Ternary => "Compute Shaders cs_5_0 YCbCr + Motion Vectors + Leaky Integrator",
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
    TernaryCodec,
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
    Ternary,
}

impl StreamCodec {
    pub fn as_arg(self) -> &'static str {
        match self {
            StreamCodec::Nvenc => "nvenc",
            StreamCodec::Ternary => "ternary",
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
    pub ooo_packets: u64,
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
    pub frames: usize,
    pub bitrate_mbps: u32,
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
    pub net_bitrate_mbps: u32,
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
            pipeline: PipelineKind::Nvenc,
            res_idx: 1, // 1920x1080
            frames: 240,
            bitrate_mbps: 25,
            fec_shards: 1,
            use_rio: true,
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
                    name: "zcstream / video_bridge",
                    subtitle: "ctest --test-dir zcstream/build (fragmentation & FEC GF(2^8))",
                    status: TestStatus::Idle,
                    duration_ms: None,
                    logs: Vec::new(),
                },
                TestEntry {
                    id: TestTargetId::TernaryCodec,
                    name: "zcvid / ternary_codec",
                    subtitle: "mingw32-make test (trit packing + codec AVX2/scalar)",
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
            net_codec: StreamCodec::Nvenc,
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
            net_bitrate_mbps: 25,
            net_fec_shards: 1,
            net_use_rio: true,
            net_stop: None,
            net_stats: NetSessionStats {
                status_msg: "Prêt à lancer zcrudp_stream.exe (capture écran DXGI + GPU).".to_owned(),
                ..Default::default()
            },
            net_logs: Vec::new(),
        }
    }
}

impl AppState {
    pub fn build_bench_args(&self) -> Vec<String> {
        let res = RESOLUTIONS[self.res_idx];
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
                args.push("--bitrate".into());
                args.push((self.bitrate_mbps as u64 * 1_000_000).to_string());
                if self.live_window {
                    args.push("--live".into());
                }
            }
            PipelineKind::Pyrowave => {
                // target-bytes per frame: bitrate_bps / (8 * 120)
                let target_bytes = (self.bitrate_mbps as u64 * 1_000_000) / (8 * 120);
                args.push("--target-bytes".into());
                args.push(target_bytes.max(1000).to_string());
            }
            PipelineKind::Ternary => {
                args.push("--bitrate".into());
                args.push((self.bitrate_mbps as u64 * 1_000_000).to_string());
                if !self.use_rio {
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
