mod net_udp;
mod runner;
mod state;
mod views;

use rune::{AppSpec, Cx, El, Tx, Waker};

use crate::state::{AppEvent, AppState};

fn setup(s: &mut AppState, waker: Waker) {
    s.waker = Some(waker.clone());
    s.devices_probing = true;
    net_udp::probe_devices(false, s.tx.clone(), Some(waker));
}

fn poll(s: &mut AppState, _t: f64, _tx: &mut Tx) -> bool {
    let mut changed = false;
    while let Ok(ev) = s.rx.try_recv() {
        changed = true;
        match ev {
            AppEvent::NetStats(stats) => {
                s.net_stats = stats;
            }
            AppEvent::NetLog(line) => {
                if s.net_logs.len() >= 300 {
                    s.net_logs.remove(0);
                }
                s.net_logs.push(line);
            }
            AppEvent::NetStopped(msg) => {
                s.net_stats.running = false;
                s.net_stats.status_msg = msg;
                s.net_stop = None;
            }
            AppEvent::ZpProbeDone { summary, logs } => {
                s.zp_probing = false;
                s.zp_probe_info = Some(summary);
                for line in logs {
                    if s.net_logs.len() >= 300 {
                        s.net_logs.remove(0);
                    }
                    s.net_logs.push(line);
                }
            }
            AppEvent::DevicesListed { catalog, logs } => {
                s.devices_probing = false;
                if catalog.loaded {
                    s.devices = catalog;
                    if !s.devices.displays.is_empty() && s.net_display_idx >= s.devices.displays.len() {
                        s.net_display_idx = 0;
                    }
                    if s.net_audio_out_idx > s.devices.audio_outs.len() {
                        s.net_audio_out_idx = 0;
                    }
                    if s.net_audio_in_idx > s.devices.audio_ins.len() {
                        s.net_audio_in_idx = 0;
                    }
                }
                for line in logs {
                    if s.net_logs.len() >= 300 {
                        s.net_logs.remove(0);
                    }
                    s.net_logs.push(line);
                }
            }
        }
    }
    changed
}

fn view(s: &AppState, _cx: &mut Cx<AppState>) -> El {
    let k = rune_theme_opencode::kit(s.dark);
    rune_kit::root(views::app_view(s, k), Vec::new())
}

fn main() {
    rune::run(AppSpec::<AppState> {
        setup: Some(setup),
        poll: Some(poll),
        min_size: Some((360.0, 420.0)),
        scenes: &[("receiver", |s| s.net_role = crate::state::NetRole::Receiver)],
        ..AppSpec::new("zcstream — Streaming Vidéo RUDP", (1200.0, 820.0), AppState::default, view)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn streaming_view_renders_cleanly() {
        let env = Env { system_dark: true, scale: 1.0 };
        let net_zcvid = AppState::default();
        let mut rt = Runtime::new(net_zcvid, view, (1200.0, 820.0), env);
        rt.frame(0.0);
        assert_eq!(rt.state.net_codec, crate::state::StreamCodec::Zcvid);
        assert!(rt.find("net_codec_seg").is_some());
        assert!(rt.find("net_hz_480").is_some());
        assert!(rt.find("net_fps_input").is_some());
        assert!(rt.find("net_bitrate_auto").is_some());
        assert!(rt.find("net_bitrate_seg").is_some());
        assert!(rt.find("net_bitrate_input").is_some());

        // Auto bandwidth scales with resolution & fps; min_mbps gates high Hz when Mbps is low.
        assert!(AppState::min_mbps_for_mode(1920, 1080, 480) > 25);
        assert!(AppState::min_mbps_for_mode(1920, 1080, 120) <= 15);
        assert!(AppState::min_mbps_for_mode(3840, 2160, 60) > 15);
        assert!(AppState::auto_bitrate_mbps(1920, 1080, 480) >= 65);
        assert!(AppState::auto_bitrate_mbps(3840, 2160, 60) >= 40);

        let net_state = AppState {
            net_connect_mode: crate::state::NetConnectMode::ZpSync,
            net_role: crate::state::NetRole::Receiver,
            net_codec: crate::state::StreamCodec::Pyrowave,
            ..AppState::default()
        };
        assert_eq!(crate::state::StreamCodec::Pyrowave.as_arg(), "pyrowave");
        let mut rt_net = Runtime::new(net_state, view, (1200.0, 820.0), env);
        rt_net.frame(0.0);
        assert!(rt_net.find("btn_zp_probe").is_some());
        assert!(rt_net.find("zp_secret_input").is_some());
        assert!(rt_net.find("net_codec_seg").is_some());
        assert!(rt_net.find("net_hz_480").is_some());
        assert!(rt_net.find("net_fps_input").is_some());

        let net_sender = AppState {
            net_role: crate::state::NetRole::Sender,
            net_codec: crate::state::StreamCodec::Nvenc,
            net_bitrate_edit: rune::TextEdit::new("15"),
            ..AppState::default()
        };
        assert_eq!(
            net_sender.net_codec.is_supported_on_platform(),
            crate::state::host_supports_nvenc()
        );
        let mut rt_sender = Runtime::new(net_sender, view, (1200.0, 820.0), env);
        rt_sender.frame(0.0);
        assert!(rt_sender.find("net_bitrate_auto").is_some());
        assert!(rt_sender.find("net_bitrate_input").is_some());

        // Platform agnosticism checks (OS/arch detection + universal Zcvid defaults)
        assert!(!crate::state::host_platform_badge().is_empty());
        assert!(crate::state::StreamCodec::Zcvid.is_supported_on_platform());
        assert_eq!(
            crate::state::StreamCodec::Pyrowave.is_supported_on_platform(),
            crate::state::host_supports_pyrowave()
        );
        assert_eq!(
            crate::state::bin_name("zcrudp_stream"),
            format!("zcrudp_stream{}", std::env::consts::EXE_SUFFIX)
        );
    }

    #[test]
    fn responsive_view_renders_across_unorthodox_viewports() {
        let env = Env { system_dark: true, scale: 1.0 };
        let viewports = [
            (420.0, 900.0),   // vertical phone / narrow portrait pane
            (640.0, 480.0),   // small 4:3 window
            (1800.0, 380.0),  // ultra-short ultra-wide ribbon
            (3440.0, 1440.0), // 21:9 ultrawide monitor
            (1200.0, 820.0),  // standard desktop
        ];
        for (w, h) in viewports {
            let mut rt_direct = Runtime::new(AppState::default(), view, (w, h), env);
            rt_direct.frame(0.0);
            assert!(rt_direct.find("header").is_some());
            assert!(rt_direct.find("net_cfg_panel").is_some());
            assert!(rt_direct.find("net_periph_panel").is_some());
            assert!(rt_direct.find("net_telemetry").is_some());
            assert!(rt_direct.find("net_kpi_periph").is_some());
            assert!(rt_direct.find("net_logs_panel").is_some());

            let zp_rx = AppState {
                net_role: crate::state::NetRole::Receiver,
                net_connect_mode: crate::state::NetConnectMode::ZpSync,
                ..AppState::default()
            };
            let mut rt_zp = Runtime::new(zp_rx, view, (w, h), env);
            rt_zp.frame(0.0);
            assert!(rt_zp.find("btn_zp_probe").is_some());
            assert!(rt_zp.find("btn_net_start").is_some());
            assert!(rt_zp.find("net_mouse_mode_seg").is_some());
        }
    }

    #[test]
    fn parse_device_list_parses_all_device_types() {
        let sample = "\
DISPLAY|0|\\\\.\\DISPLAY1 (Principal)|1920x1080|0,0\n\
DISPLAY|1|\\\\.\\DISPLAY2|1440x2560|-1440,0\n\
AUDIO_OUT|0|Speakers (Minifuse 1)\n\
AUDIO_IN|0|Input 1 (Minifuse 1)\n\
GAMEPAD|0|XInput Controller #1\n";
        let cat = net_udp::parse_device_list(sample);
        assert!(cat.loaded);
        assert_eq!(cat.displays.len(), 2);
        assert_eq!(cat.displays[0].id, "0");
        assert!(cat.displays[1].label.contains("1440x2560"));
        assert_eq!(cat.audio_outs.len(), 1);
        assert_eq!(cat.audio_ins.len(), 1);
        assert_eq!(cat.gamepads.len(), 1);
        assert_eq!(cat.gamepads[0].label, "XInput Controller #1");
    }

    #[test]
    fn stream_args_include_peripheral_flags() {
        let periph = net_udp::PeriphLaunchOpts {
            display_idx: 1,
            input: true,
            mouse_relative: true,
            gamepad: true,
            audio: true,
            mic: true,
            audio_out: "2".to_owned(),
            audio_in: "1".to_owned(),
        };
        let tx_args = net_udp::build_stream_args(
            crate::state::NetRole::Sender,
            crate::state::NetConnectMode::DirectUdp,
            crate::state::StreamCodec::Zcvid,
            1920,
            1080,
            "192.168.1.50",
            9900,
            None,
            120,
            0,
            1,
            false,
            &periph,
        );
        assert!(tx_args.windows(2).any(|w| w == ["--display", "1"]));
        assert!(tx_args.contains(&"--input".to_owned()));
        assert!(tx_args.contains(&"--gamepad".to_owned()));
        assert!(tx_args.contains(&"--audio".to_owned()));
        assert!(tx_args.contains(&"--mic".to_owned()));
        assert!(tx_args.windows(2).any(|w| w == ["--audio-out", "2"]));

        let rx_args = net_udp::build_stream_args(
            crate::state::NetRole::Receiver,
            crate::state::NetConnectMode::DirectUdp,
            crate::state::StreamCodec::Zcvid,
            1920,
            1080,
            "127.0.0.1",
            9900,
            None,
            120,
            0,
            1,
            false,
            &periph,
        );
        assert!(rx_args.windows(2).any(|w| w == ["--mouse-mode", "rel"]));
        assert!(rx_args.windows(2).any(|w| w == ["--audio-in", "1"]));
    }

    #[test]
    fn binary_and_repo_discovery_is_portable() {
        let base = std::env::temp_dir().join(format!(
            "zcstream_app_disc_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let exe_dir = base.join("portable");
        let root = base.join("ws");
        let msvc_rel = root.join("zcstream").join("build").join("Release");
        let deps_zcvid = root.join("zcstream").join("build").join("_deps").join("zcvid-src");
        let env_dir = base.join("override");

        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&msvc_rel).unwrap();
        std::fs::create_dir_all(&deps_zcvid).unwrap();
        std::fs::create_dir_all(&env_dir).unwrap();

        let stream_name = crate::state::bin_name("zcrudp_stream");
        let msvc_bin = msvc_rel.join(&stream_name);
        std::fs::write(&msvc_bin, b"stub").unwrap();

        // 1. Multi-config MSVC Release/ subdir is discovered automatically
        assert_eq!(
            crate::state::find_binary_in("zcrudp_stream", None, Some(&exe_dir), &root, None),
            Some(msvc_bin.clone())
        );

        // 2. Portable bundle next to executable takes priority over workspace build dir
        let portable_bin = exe_dir.join(&stream_name);
        std::fs::write(&portable_bin, b"stub").unwrap();
        assert_eq!(
            crate::state::find_binary_in("zcrudp_stream", None, Some(&exe_dir), &root, None),
            Some(portable_bin.clone())
        );

        // 3. Explicit ZCSTREAM_BIN_DIR overrides everything
        let override_bin = env_dir.join(&stream_name);
        std::fs::write(&override_bin, b"stub").unwrap();
        assert_eq!(
            crate::state::find_binary_in("zcrudp_stream", Some(&env_dir), Some(&exe_dir), &root, None),
            Some(override_bin)
        );

        // 4. Missing binary returns None cleanly
        assert_eq!(
            crate::state::find_binary_in("nonexistent_tool_xyz", None, Some(&exe_dir), &root, None),
            None
        );

        // 5. Repo discovery falls back to CMake auto-fetched _deps/<name>-src
        assert_eq!(
            crate::state::find_repo_dir_in("zcvid", &root, Some(&exe_dir)),
            deps_zcvid
        );

        // 6. GitHub ZIP download (<repo>-main) takes priority over _deps
        let zip_zcvid = root.join("zcvid-main");
        std::fs::create_dir_all(&zip_zcvid).unwrap();
        assert_eq!(
            crate::state::find_repo_dir_in("zcvid", &root, Some(&exe_dir)),
            zip_zcvid
        );

        // 7. Direct sibling repo in workspace root takes priority over <repo>-main and _deps
        let sibling_zcvid = root.join("zcvid");
        std::fs::create_dir_all(&sibling_zcvid).unwrap();
        assert_eq!(
            crate::state::find_repo_dir_in("zcvid", &root, Some(&exe_dir)),
            sibling_zcvid
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
