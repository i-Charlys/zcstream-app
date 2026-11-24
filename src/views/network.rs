use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rune::{El, ScrollSpec, TextEdit, col, row, text};
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, ControlSize, Divider, Kit, SegmentedControl,
    Spinner, Switch, TextInput,
};

use crate::net_udp::{
    PeriphLaunchOpts, ZpSyncLaunchOpts, probe_devices, probe_zpsync_network, start_network_session,
};
use crate::state::{
    AppState, NetConnectMode, NetRole, RESOLUTIONS, StreamCodec, bin_name, host_platform_badge,
    host_supports_rio,
};
use crate::views::{kpi_box, panel, stepper};

pub fn view(s: &AppState, k: &Kit) -> El {
    let role_idx = match s.net_role {
        NetRole::Sender => 0,
        NetRole::Receiver => 1,
    };
    let mode_idx = match s.net_connect_mode {
        NetConnectMode::DirectUdp => 0,
        NetConnectMode::ZpSync => 1,
    };
    let codec_idx = match s.net_codec {
        StreamCodec::Nvenc => 0,
        StreamCodec::Pyrowave => 1,
        StreamCodec::Zcvid => 2,
    };

    let (net_w, net_h) = s.effective_net_dims();
    let net_cur_fps = s.effective_net_fps();
    let net_is_auto_bw = s.is_net_bitrate_auto();
    let net_cur_mbps = s.effective_net_bitrate_mbps();
    let net_auto_mbps = AppState::auto_bitrate_mbps(net_w, net_h, net_cur_fps);
    let net_min_cur_mbps = AppState::min_mbps_for_mode(net_w, net_h, net_cur_fps);
    let net_bw_limited = s.net_role == NetRole::Sender && !net_is_auto_bw;
    let net_fps_under_bw = net_bw_limited && net_cur_mbps < net_min_cur_mbps;

    let mut net_hz_row = row()
        .w_full()
        .flex_wrap()
        .center_y()
        .gap(8.0)
        .child(
            text("Cadence (Hz)")
                .font(k.ty.ui)
                .color(k.c.text_base)
                .min_w(108.0),
        );
    for (hz, key) in [
        (24_u32, "net_hz_24"),
        (60_u32, "net_hz_60"),
        (120_u32, "net_hz_120"),
        (240_u32, "net_hz_240"),
        (480_u32, "net_hz_480"),
    ] {
        let enough_bw = !net_bw_limited || net_cur_mbps >= AppState::min_mbps_for_mode(net_w, net_h, hz);
        let selected = net_cur_fps == hz && enough_bw;
        net_hz_row = net_hz_row.child(
            Button::new(format!("{hz} Hz"))
                .variant(if selected {
                    ButtonVariant::Submit
                } else {
                    ButtonVariant::Outline
                })
                .control_size(ControlSize::Small)
                .disabled(s.net_stats.running || !enough_bw)
                .on_press(move |s: &mut AppState| {
                    if !s.net_stats.running {
                        s.net_fps = hz;
                        s.net_fps_edit = TextEdit::new(hz.to_string());
                    }
                })
                .build(k)
                .key(key),
        );
    }
    net_hz_row = net_hz_row.child(
        row()
            .center_y()
            .gap(6.0)
            .child(
                TextInput::new(&s.net_fps_edit)
                    .placeholder("120")
                    .width(Some(64.0))
                    .on_edit(|s: &mut AppState| &mut s.net_fps_edit)
                    .build(k)
                    .key("net_fps_input"),
            )
            .child(text("Hz").font(k.ty.small).color(k.c.text_muted)),
    );

    let net_codec_supported = s.net_codec.is_supported_on_platform();
    let stream_exe = bin_name("zcrudp_stream");
    let capture_subtitle = if cfg!(target_os = "windows") {
        "Capture DXGI Desktop Duplication + encodage GPU + transport zcrudp (2 PC ou loopback)"
    } else {
        "Capture X11 desktop (fallback mire sous Wayland/headless) + encodage Zcvid Portable + transport UDP zcrudp"
    };

    let config_panel = panel(
        k,
        "net_cfg_panel",
        &format!("Streaming Temps Réel ({stream_exe})"),
        Some(capture_subtitle),
        col()
            .w_full()
            .min_w(0.0)
            .gap(12.0)
            .child(
                row()
                    .w_full()
                    .flex_wrap()
                    .justify_between()
                    .center_y()
                    .gap(10.0)
                    .child(
                        row()
                            .flex_wrap()
                            .center_y()
                            .gap(10.0)
                            .child(text("Rôle").font(k.ty.ui).color(k.c.text_base).min_w(96.0))
                            .child(
                                SegmentedControl::new(
                                    &["Émetteur (Capture écran)", "Récepteur (Fenêtre live)"],
                                    Some(role_idx),
                                )
                                .fit()
                                .on_pick(|s: &mut AppState, idx, _| {
                                    if !s.net_stats.running {
                                        s.net_role = if idx == 0 {
                                            NetRole::Sender
                                        } else {
                                            NetRole::Receiver
                                        };
                                    }
                                })
                                .build(k)
                                .key("net_role_seg"),
                            ),
                    )
                    .child(
                        Badge::new(if s.net_stats.running { "Actif" } else { "Inactif" })
                            .variant(if s.net_stats.running {
                                BadgeVariant::Accent
                            } else {
                                BadgeVariant::Neutral
                            })
                            .build(k),
                    ),
            )
            .child(
                row()
                    .w_full()
                    .flex_wrap()
                    .center_y()
                    .gap(10.0)
                    .child(text("Codec").font(k.ty.ui).color(k.c.text_base).min_w(96.0))
                    .child(
                        SegmentedControl::new(
                            &[
                                "NVENC (H.264)",
                                "Pyrowave (Wavelets)",
                                if cfg!(target_os = "windows") {
                                    "Zcvid Unified"
                                } else {
                                    "Zcvid Portable"
                                },
                            ],
                            Some(codec_idx),
                        )
                        .fit()
                        .on_pick(|s: &mut AppState, idx, _| {
                            if !s.net_stats.running {
                                s.net_codec = match idx {
                                    0 => StreamCodec::Nvenc,
                                    1 => StreamCodec::Pyrowave,
                                    _ => StreamCodec::Zcvid,
                                };
                            }
                        })
                        .build(k)
                        .key("net_codec_seg"),
                    ),
            )
            .child(
                row()
                    .w_full()
                    .flex_wrap()
                    .center_y()
                    .gap(10.0)
                    .child(text("Résolution").font(k.ty.ui).color(k.c.text_base).min_w(96.0))
                    .child(
                        SegmentedControl::new(
                            &["720p", "1080p", "1440p", "4K"],
                            s.net_res_idx,
                        )
                        .fit()
                        .on_pick(|s: &mut AppState, idx, _| {
                            if !s.net_stats.running {
                                let i = idx.min(RESOLUTIONS.len() - 1);
                                s.net_res_idx = Some(i);
                                s.net_width_edit = TextEdit::new(RESOLUTIONS[i].width.to_string());
                                s.net_height_edit = TextEdit::new(RESOLUTIONS[i].height.to_string());
                            }
                        })
                        .build(k)
                        .key("net_res_seg"),
                    )
                    .child(
                        row()
                            .center_y()
                            .gap(6.0)
                            .child(
                                TextInput::new(&s.net_width_edit)
                                    .placeholder("1920")
                                    .width(Some(72.0))
                                    .on_edit(|s: &mut AppState| &mut s.net_width_edit)
                                    .build(k)
                                    .key("net_w_input"),
                            )
                            .child(text("×").font(k.ty.ui).color(k.c.text_muted))
                            .child(
                                TextInput::new(&s.net_height_edit)
                                    .placeholder("1080")
                                    .width(Some(72.0))
                                    .on_edit(|s: &mut AppState| &mut s.net_height_edit)
                                    .build(k)
                                    .key("net_h_input"),
                            ),
                    ),
            )
            .child(
                row()
                    .w_full()
                    .flex_wrap()
                    .justify_between()
                    .center_y()
                    .gap(10.0)
                    .child(
                        row()
                            .flex_wrap()
                            .center_y()
                            .gap(10.0)
                            .child(text("Transport").font(k.ty.ui).color(k.c.text_base).min_w(96.0))
                            .child(
                                SegmentedControl::new(
                                    &["Direct UDP (LAN / IP)", "ZPSync P2P (NAT Traversal)"],
                                    Some(mode_idx),
                                )
                                .fit()
                                .on_pick(|s: &mut AppState, idx, _| {
                                    if !s.net_stats.running {
                                        s.net_connect_mode = if idx == 0 {
                                            NetConnectMode::DirectUdp
                                        } else {
                                            NetConnectMode::ZpSync
                                        };
                                    }
                                })
                                .build(k)
                                .key("net_mode_seg"),
                            ),
                    )
                    .child(if s.net_connect_mode == NetConnectMode::ZpSync {
                        row()
                            .flex_wrap()
                            .center_y()
                            .gap(8.0)
                            .child(if let Some(ref info) = s.zp_probe_info {
                                text(info.clone()).font(k.ty.small).color(k.c.text_muted).wrap()
                            } else {
                                rune::el().size(0.0, 0.0)
                            })
                            .child(
                                Button::new(if s.zp_probing {
                                    "Sondage..."
                                } else {
                                    "Sonder réseau (STUN / IPv6)"
                                })
                                .variant(ButtonVariant::Outline)
                                .control_size(ControlSize::Regular)
                                .disabled(s.zp_probing || s.net_stats.running)
                                .on_press(|s: &mut AppState| {
                                    s.zp_probing = true;
                                    let lp: u16 = s.zp_local_port_edit.text.trim().parse().unwrap_or(0);
                                    probe_zpsync_network(lp, s.tx.clone(), s.waker.clone());
                                })
                                .build(k)
                                .key("btn_zp_probe"),
                            )
                    } else {
                        rune::el().size(0.0, 0.0)
                    }),
            )
            .child(if s.net_connect_mode == NetConnectMode::DirectUdp {
                row()
                    .w_full()
                    .flex_wrap()
                    .center_y()
                    .gap(10.0)
                    .child(
                        text(if s.net_role == NetRole::Sender {
                            "IP Cible"
                        } else {
                            "Écoute sur"
                        })
                        .font(k.ty.ui)
                        .color(k.c.text_base)
                        .min_w(96.0),
                    )
                    .child(if s.net_role == NetRole::Sender {
                        TextInput::new(&s.net_host_edit)
                            .placeholder("192.168.1.50")
                            .width(Some(200.0))
                            .on_edit(|s: &mut AppState| &mut s.net_host_edit)
                            .build(k)
                            .key("net_host_input")
                    } else {
                        text("0.0.0.0 (toutes interfaces)")
                            .font(k.ty.mono)
                            .color(k.c.text_muted)
                            .wrap()
                    })
                    .child(
                        row()
                            .center_y()
                            .gap(8.0)
                            .child(text("Port UDP").font(k.ty.ui).color(k.c.text_base))
                            .child(
                                TextInput::new(&s.net_port_edit)
                                    .placeholder("9900")
                                    .width(Some(96.0))
                                    .on_edit(|s: &mut AppState| &mut s.net_port_edit)
                                    .build(k)
                                    .key("net_port_input"),
                            ),
                    )
            } else {
                col()
                    .w_full()
                    .gap(8.0)
                    .child(
                        row()
                            .w_full()
                            .flex_wrap()
                            .center_y()
                            .gap(10.0)
                            .child(
                                row()
                                    .center_y()
                                    .gap(8.0)
                                    .child(text("Secret (S)").font(k.ty.ui).color(k.c.text_base).min_w(96.0))
                                    .child(
                                        TextInput::new(&s.zp_secret_edit)
                                            .placeholder("zcstream-p2p")
                                            .width(Some(190.0))
                                            .on_edit(|s: &mut AppState| &mut s.zp_secret_edit)
                                            .build(k)
                                            .key("zp_secret_input"),
                                    ),
                            )
                            .child(
                                row()
                                    .center_y()
                                    .gap(8.0)
                                    .child(text("IP Pair (opt.)").font(k.ty.ui).color(k.c.text_base))
                                    .child(
                                        TextInput::new(&s.zp_peer_ip_edit)
                                            .placeholder("Auto (DHT)")
                                            .width(Some(150.0))
                                            .on_edit(|s: &mut AppState| &mut s.zp_peer_ip_edit)
                                            .build(k)
                                            .key("zp_peer_ip_input"),
                                    ),
                            )
                            .child(
                                row()
                                    .center_y()
                                    .gap(8.0)
                                    .child(text("Port pair").font(k.ty.ui).color(k.c.text_base))
                                    .child(
                                        TextInput::new(&s.zp_peer_port_edit)
                                            .placeholder("0")
                                            .width(Some(68.0))
                                            .on_edit(|s: &mut AppState| &mut s.zp_peer_port_edit)
                                            .build(k)
                                            .key("zp_peer_port_input"),
                                    ),
                            )
                            .child(
                                row()
                                    .center_y()
                                    .gap(8.0)
                                    .child(text("Local").font(k.ty.ui).color(k.c.text_base))
                                    .child(
                                        TextInput::new(&s.zp_local_port_edit)
                                            .placeholder("0")
                                            .width(Some(68.0))
                                            .on_edit(|s: &mut AppState| &mut s.zp_local_port_edit)
                                            .build(k)
                                            .key("zp_local_port_input"),
                                    ),
                            ),
                    )
                    .child(
                        row()
                            .w_full()
                            .flex_wrap()
                            .center_y()
                            .gap(16.0)
                            .child(
                                Switch::new(s.zp_sentinel)
                                    .label("Sentinel ICMP (IPv4 Layer-3)")
                                    .on_toggle(|s: &mut AppState| s.zp_sentinel = !s.zp_sentinel)
                                    .build(k)
                                    .key("zp_sw_sentinel"),
                            )
                            .child(
                                Switch::new(s.zp_birthday)
                                    .label("Forcer Birthday Paradox (CGNAT 256-500 sockets)")
                                    .on_toggle(|s: &mut AppState| s.zp_birthday = !s.zp_birthday)
                                    .build(k)
                                    .key("zp_sw_birthday"),
                            ),
                    )
            })
            .child(Divider::horizontal().build(k))
            .child(if s.net_role == NetRole::Sender {
                col()
                    .w_full()
                    .gap(8.0)
                    .child(
                        row()
                            .w_full()
                            .flex_wrap()
                            .center_y()
                            .gap(8.0)
                            .child(
                                text("Bande passante")
                                    .font(k.ty.ui)
                                    .color(k.c.text_base)
                                    .min_w(108.0),
                            )
                            .child(
                                Button::new("Auto")
                                    .variant(if net_is_auto_bw {
                                        ButtonVariant::Submit
                                    } else {
                                        ButtonVariant::Outline
                                    })
                                    .control_size(ControlSize::Small)
                                    .disabled(s.net_stats.running)
                                    .on_press(|s: &mut AppState| {
                                        if !s.net_stats.running {
                                            s.net_bitrate_edit = TextEdit::new("");
                                        }
                                    })
                                    .build(k)
                                    .key("net_bitrate_auto"),
                            )
                            .child(
                                SegmentedControl::new(
                                    &["15", "25", "50", "100", "200"],
                                    if net_is_auto_bw {
                                        None
                                    } else {
                                        match net_cur_mbps {
                                            15 => Some(0),
                                            25 => Some(1),
                                            50 => Some(2),
                                            100 => Some(3),
                                            200 => Some(4),
                                            _ => None,
                                        }
                                    },
                                )
                                .fit()
                                .on_pick(|s: &mut AppState, idx, _| {
                                    if !s.net_stats.running {
                                        let mbps = match idx {
                                            0 => 15,
                                            1 => 25,
                                            2 => 50,
                                            3 => 100,
                                            _ => 200,
                                        };
                                        s.net_bitrate_mbps = mbps;
                                        s.net_bitrate_edit = TextEdit::new(mbps.to_string());
                                    }
                                })
                                .build(k)
                                .key("net_bitrate_seg"),
                            )
                            .child(
                                row()
                                    .center_y()
                                    .gap(6.0)
                                    .child(
                                        TextInput::new(&s.net_bitrate_edit)
                                            .placeholder(format!("auto ({net_auto_mbps})"))
                                            .width(Some(76.0))
                                            .on_edit(|s: &mut AppState| &mut s.net_bitrate_edit)
                                            .build(k)
                                            .key("net_bitrate_input"),
                                    )
                                    .child(text("Mbps").font(k.ty.small).color(k.c.text_muted)),
                            ),
                    )
                    .child(net_hz_row)
                    .child(if net_fps_under_bw {
                        text(format!(
                            "Débit insuffisant pour {}×{} @ {} Hz (min. {} Mbps) — cliquez Auto ou augmentez les Mbps.",
                            net_w, net_h, net_cur_fps, net_min_cur_mbps
                        ))
                        .font(k.ty.small)
                        .color(k.c.text_accent)
                        .wrap()
                    } else {
                        rune::el().size(0.0, 0.0)
                    })
                    .child(stepper(
                        k,
                        "net_fec",
                        "Parité FEC Cauchy GF(2⁸)",
                        if s.net_fec_shards == 0 {
                            "Off".to_owned()
                        } else {
                            format!("{} shard(s)", s.net_fec_shards)
                        },
                        |s| s.net_fec_shards = s.net_fec_shards.saturating_sub(1),
                        |s| s.net_fec_shards = (s.net_fec_shards + 1).min(4),
                    ))
                    .child(if host_supports_rio() {
                        Switch::new(s.net_use_rio)
                            .label("Winsock RIO (Registered I/O)")
                            .description("Envoi UDP zéro-syscall sur l'émetteur")
                            .on_toggle(|s: &mut AppState| s.net_use_rio = !s.net_use_rio)
                            .build(k)
                            .key("net_sw_rio")
                    } else {
                        text("Transport UDP POSIX actif (Winsock RIO spécifique Windows).")
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .wrap()
                    })
            } else {
                col()
                    .w_full()
                    .gap(8.0)
                    .child(net_hz_row)
                    .child(
                        text(if cfg!(target_os = "windows") {
                            "Le récepteur ouvre une fenêtre FLIP_DISCARD temps réel dès réception des frames."
                        } else {
                            "Le récepteur ouvre une fenêtre X11 temps réel dès réception (ou décode en headless si DISPLAY est absent)."
                        })
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .wrap(),
                    )
            })
            .child(
                row()
                    .w_full()
                    .flex_wrap()
                    .center_y()
                    .gap(12.0)
                    .pt(6.0)
                    .child(if s.net_stats.running {
                        Button::new("Arrêter zcrudp_stream")
                            .variant(ButtonVariant::Danger)
                            .control_size(ControlSize::Large)
                            .on_press(|s: &mut AppState| {
                                if let Some(stop) = &s.net_stop {
                                    stop.store(true, Ordering::Relaxed);
                                }
                            })
                            .build(k)
                            .key("btn_net_stop")
                    } else {
                        Button::new(if s.net_role == NetRole::Sender {
                            "Démarrer la capture & émission"
                        } else {
                            "Démarrer la réception & affichage"
                        })
                        .variant(ButtonVariant::Submit)
                        .control_size(ControlSize::Large)
                        .disabled(!net_codec_supported)
                        .on_press(|s: &mut AppState| {
                            if !s.net_codec.is_supported_on_platform() {
                                return;
                            }
                            let port: u16 = s.net_port_edit.text.trim().parse().unwrap_or(9900);
                            let width: u32 = s.net_width_edit.text.trim().parse().unwrap_or(1920);
                            let height: u32 = s.net_height_edit.text.trim().parse().unwrap_or(1080);
                            let host = s.net_host_edit.text.trim().to_owned();
                            let zp_opts = if s.net_connect_mode == NetConnectMode::ZpSync {
                                Some(ZpSyncLaunchOpts {
                                    secret: s.zp_secret_edit.text.trim().to_owned(),
                                    peer_ip: s.zp_peer_ip_edit.text.trim().to_owned(),
                                    peer_port: s.zp_peer_port_edit.text.trim().parse().unwrap_or(0),
                                    local_port: s.zp_local_port_edit.text.trim().parse().unwrap_or(0),
                                    birthday: s.zp_birthday,
                                    sentinel: s.zp_sentinel,
                                })
                            } else {
                                None
                            };
                            let stop = Arc::new(AtomicBool::new(false));
                            s.net_stop = Some(stop.clone());
                            s.net_stats.running = true;
                            s.net_stats.zp_path.clear();
                            s.net_stats.zp_session.clear();
                            s.net_stats.zp_rtt_ms = None;
                            s.net_logs.clear();
                            s.net_stats.status_msg = format!(
                                "Lancement de {} ({})...",
                                bin_name("zcrudp_stream"),
                                host_platform_badge()
                            );
                            let fps = s.effective_net_fps();
                            let bitrate_mbps = if s.is_net_bitrate_auto() {
                                if matches!(s.net_codec, StreamCodec::Zcvid | StreamCodec::Pyrowave) {
                                    0 // let zcrudp_stream compute the exact auto bitrate
                                } else {
                                    AppState::auto_bitrate_mbps(width, height, fps)
                                }
                            } else {
                                s.effective_net_bitrate_mbps()
                            };
                            let display_idx = s
                                .devices
                                .displays
                                .get(s.net_display_idx)
                                .and_then(|d| d.id.parse::<u32>().ok())
                                .unwrap_or(s.net_display_idx as u32);
                            let audio_out = if s.net_audio_out_idx == 0 {
                                "default".to_owned()
                            } else {
                                s.devices
                                    .audio_outs
                                    .get(s.net_audio_out_idx - 1)
                                    .map(|d| d.id.clone())
                                    .unwrap_or_else(|| "default".to_owned())
                            };
                            let audio_in = if s.net_audio_in_idx == 0 {
                                "default".to_owned()
                            } else {
                                s.devices
                                    .audio_ins
                                    .get(s.net_audio_in_idx - 1)
                                    .map(|d| d.id.clone())
                                    .unwrap_or_else(|| "default".to_owned())
                            };
                            let periph = PeriphLaunchOpts {
                                display_idx,
                                input: s.net_input_enabled,
                                mouse_relative: s.net_mouse_relative,
                                gamepad: s.net_gamepad_enabled,
                                audio: s.net_audio_enabled,
                                mic: s.net_mic_enabled,
                                audio_out,
                                audio_in,
                            };
                            s.net_fps = fps;
                            s.net_bitrate_mbps = bitrate_mbps;
                            start_network_session(
                                s.net_role,
                                s.net_connect_mode,
                                s.net_codec,
                                width,
                                height,
                                host,
                                port,
                                zp_opts,
                                fps,
                                bitrate_mbps,
                                s.net_fec_shards,
                                s.net_use_rio,
                                periph,
                                s.tx.clone(),
                                s.waker.clone(),
                                stop,
                            );
                        })
                        .build(k)
                        .key("btn_net_start")
                    })
                    .child(if s.net_stats.running {
                        Spinner::new().size(16.0).color(k.c.bg_accent).build(k)
                    } else {
                        rune::el().size(0.0, 0.0)
                    })
                    .child(
                        col()
                            .min_w(0.0)
                            .shrink()
                            .child(if !net_codec_supported {
                                text("Ce codec requiert Windows x64 GPU — sélectionnez Zcvid Portable sur cette plateforme.")
                                    .font(k.ty.ui)
                                    .color(k.c.text_accent)
                                    .wrap()
                            } else {
                                text(s.net_stats.status_msg.clone())
                                    .font(k.ty.ui)
                                    .color(k.c.text_muted)
                                    .wrap()
                            }),
                    ),
            ),
    );

    let display_label = if s.devices.displays.is_empty() {
        format!("Écran #{} (principal)", s.net_display_idx)
    } else {
        s.devices
            .displays
            .get(s.net_display_idx)
            .map(|d| d.label.clone())
            .unwrap_or_else(|| format!("Écran #{}", s.net_display_idx))
    };
    let audio_out_label = if s.net_audio_out_idx == 0 {
        "Par défaut (système)".to_owned()
    } else {
        s.devices
            .audio_outs
            .get(s.net_audio_out_idx - 1)
            .map(|d| d.label.clone())
            .unwrap_or_else(|| "Par défaut".to_owned())
    };
    let audio_in_label = if s.net_audio_in_idx == 0 {
        "Par défaut (micro)".to_owned()
    } else {
        s.devices
            .audio_ins
            .get(s.net_audio_in_idx - 1)
            .map(|d| d.label.clone())
            .unwrap_or_else(|| "Par défaut".to_owned())
    };
    let disp_count = s.devices.displays.len();
    let audio_out_count = s.devices.audio_outs.len();
    let audio_in_count = s.devices.audio_ins.len();

    let periph_panel = panel(
        k,
        "net_periph_panel",
        "Périphériques (Entrées & Sorties)",
        Some("Capture/injection clavier, souris, manettes (ViGEm/uinput) et audio bidirectionnel 48 kHz"),
        col()
            .w_full()
            .min_w(0.0)
            .gap(12.0)
            .child(
                row()
                    .w_full()
                    .flex_wrap()
                    .justify_between()
                    .center_y()
                    .gap(8.0)
                    .child(
                        row()
                            .flex_wrap()
                            .center_y()
                            .gap(6.0)
                            .child(
                                Badge::new(format!("{} écran(s)", disp_count.max(1)))
                                    .variant(BadgeVariant::Neutral)
                                    .build(k),
                            )
                            .child(
                                Badge::new(format!(
                                    "{} sortie(s) / {} micro(s)",
                                    audio_out_count, audio_in_count
                                ))
                                .variant(BadgeVariant::Neutral)
                                .build(k),
                            )
                            .child(
                                Badge::new(if s.devices.gamepads.is_empty() {
                                    "0 manette détectée".to_owned()
                                } else {
                                    format!("{} manette(s)", s.devices.gamepads.len())
                                })
                                .variant(if s.devices.gamepads.is_empty() {
                                    BadgeVariant::Neutral
                                } else {
                                    BadgeVariant::Accent
                                })
                                .build(k),
                            ),
                    )
                    .child(
                        Button::new(if s.devices_probing {
                            "Détection..."
                        } else {
                            "Rafraîchir périphériques"
                        })
                        .variant(ButtonVariant::Outline)
                        .control_size(ControlSize::Small)
                        .disabled(s.devices_probing || s.net_stats.running)
                        .on_press(|s: &mut AppState| {
                            s.devices_probing = true;
                            probe_devices(true, s.tx.clone(), s.waker.clone());
                        })
                        .build(k)
                        .key("btn_periph_refresh"),
                    ),
            )
            .child(
                row()
                    .w_full()
                    .flex_wrap()
                    .gap(16.0)
                    .child(
                        Switch::new(s.net_input_enabled)
                            .label("Clavier & Souris")
                            .description(if s.net_role == NetRole::Sender {
                                "Injecte clavier/souris reçus sur cet hôte"
                            } else {
                                "Envoie clavier & souris depuis la fenêtre vidéo"
                            })
                            .on_toggle(|s: &mut AppState| {
                                if !s.net_stats.running {
                                    s.net_input_enabled = !s.net_input_enabled;
                                }
                            })
                            .build(k)
                            .key("net_sw_input"),
                    )
                    .child(
                        Switch::new(s.net_gamepad_enabled)
                            .label("Manettes / Gamepad")
                            .description(if s.net_role == NetRole::Sender {
                                "Émule Xbox 360 virtuelle (ViGEmBus / uinput)"
                            } else {
                                "Lit XInput / /dev/input/js* vers l'hôte"
                            })
                            .on_toggle(|s: &mut AppState| {
                                if !s.net_stats.running {
                                    s.net_gamepad_enabled = !s.net_gamepad_enabled;
                                }
                            })
                            .build(k)
                            .key("net_sw_gamepad"),
                    )
                    .child(
                        Switch::new(s.net_audio_enabled)
                            .label("Audio système (Hôte → Client)")
                            .description("Son bureau 48 kHz IMA-ADPCM faible latence")
                            .on_toggle(|s: &mut AppState| {
                                if !s.net_stats.running {
                                    s.net_audio_enabled = !s.net_audio_enabled;
                                }
                            })
                            .build(k)
                            .key("net_sw_audio"),
                    )
                    .child(
                        Switch::new(s.net_mic_enabled)
                            .label("Microphone (Client → Hôte)")
                            .description("Envoie la voix du client vers la sortie hôte")
                            .on_toggle(|s: &mut AppState| {
                                if !s.net_stats.running {
                                    s.net_mic_enabled = !s.net_mic_enabled;
                                }
                            })
                            .build(k)
                            .key("net_sw_mic"),
                    ),
            )
            .child(if s.net_role == NetRole::Receiver && s.net_input_enabled {
                col()
                    .w_full()
                    .min_w(0.0)
                    .gap(6.0)
                    .child(
                        row()
                            .w_full()
                            .flex_wrap()
                            .center_y()
                            .gap(10.0)
                            .child(
                                text("Mode Souris")
                                    .font(k.ty.ui)
                                    .color(k.c.text_base)
                                    .min_w(96.0),
                            )
                            .child(
                                SegmentedControl::new(
                                    &["Absolu (Bureau)", "Relatif (FPS)"],
                                    Some(if s.net_mouse_relative { 1 } else { 0 }),
                                )
                                .fit()
                                .on_pick(|s: &mut AppState, idx, _| {
                                    if !s.net_stats.running {
                                        s.net_mouse_relative = idx == 1;
                                    }
                                })
                                .build(k)
                                .key("net_mouse_mode_seg"),
                            ),
                    )
                    .child(
                        text("Raccourcis fenêtre : Ctrl+Alt+Shift+M (verrou FPS), Ctrl+Alt+Shift+Q (fermer).")
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .wrap(),
                    )
            } else {
                rune::el().size(0.0, 0.0)
            })
            .child(if s.net_role == NetRole::Sender {
                stepper(
                    k,
                    "periph_display",
                    "Écran capturé",
                    display_label,
                    move |s| {
                        if !s.net_stats.running && disp_count > 1 {
                            s.net_display_idx = if s.net_display_idx == 0 {
                                disp_count - 1
                            } else {
                                s.net_display_idx - 1
                            };
                        }
                    },
                    move |s| {
                        if !s.net_stats.running && disp_count > 1 {
                            s.net_display_idx = (s.net_display_idx + 1) % disp_count;
                        }
                    },
                )
            } else {
                rune::el().size(0.0, 0.0)
            })
            .child(if s.net_audio_enabled || (s.net_role == NetRole::Sender && s.net_mic_enabled) {
                stepper(
                    k,
                    "periph_audio_out",
                    if s.net_role == NetRole::Sender {
                        "Audio hôte (loopback / sortie)"
                    } else {
                        "Sortie audio locale"
                    },
                    audio_out_label,
                    move |s| {
                        if !s.net_stats.running {
                            let max_n = audio_out_count + 1;
                            s.net_audio_out_idx = if s.net_audio_out_idx == 0 {
                                max_n - 1
                            } else {
                                s.net_audio_out_idx - 1
                            };
                        }
                    },
                    move |s| {
                        if !s.net_stats.running {
                            let max_n = audio_out_count + 1;
                            s.net_audio_out_idx = (s.net_audio_out_idx + 1) % max_n;
                        }
                    },
                )
            } else {
                rune::el().size(0.0, 0.0)
            })
            .child(if s.net_role == NetRole::Receiver && s.net_mic_enabled {
                stepper(
                    k,
                    "periph_audio_in",
                    "Microphone local",
                    audio_in_label,
                    move |s| {
                        if !s.net_stats.running {
                            let max_n = audio_in_count + 1;
                            s.net_audio_in_idx = if s.net_audio_in_idx == 0 {
                                max_n - 1
                            } else {
                                s.net_audio_in_idx - 1
                            };
                        }
                    },
                    move |s| {
                        if !s.net_stats.running {
                            let max_n = audio_in_count + 1;
                            s.net_audio_in_idx = (s.net_audio_in_idx + 1) % max_n;
                        }
                    },
                )
            } else {
                rune::el().size(0.0, 0.0)
            }),
    );

    let telemetry = panel(
        k,
        "net_telemetry",
        "Télémétrie Temps Réel",
        None,
        row()
            .w_full()
            .flex_wrap()
            .gap(12.0)
            .child(kpi_box(
                k,
                "net_kpi_fps",
                "Cadence mesurée",
                format!("{:.1} fps", s.net_stats.fps),
                Some(k.c.success.fg),
            ))
            .child(kpi_box(
                k,
                "net_kpi_mbps",
                "Débit mesuré",
                format!("{:.2} Mbps", s.net_stats.mbps),
                Some(k.c.text_accent),
            ))
            .child(kpi_box(
                k,
                "net_kpi_frames",
                if s.net_role == NetRole::Sender {
                    "Frames envoyées"
                } else {
                    "Frames décodées"
                },
                if s.net_role == NetRole::Sender {
                    format!("{}", s.net_stats.frames_tx)
                } else {
                    format!("{}", s.net_stats.frames_rx)
                },
                None,
            ))
            .child(kpi_box(
                k,
                "net_kpi_extra",
                if s.net_role == NetRole::Sender {
                    "Paquets TX"
                } else {
                    "FEC récupérées / Perdues"
                },
                if s.net_role == NetRole::Sender {
                    format!("{}", s.net_stats.packets_total)
                } else {
                    format!("{} / {}", s.net_stats.fec_recovered, s.net_stats.frames_dropped)
                },
                None,
            ))
            .child(kpi_box(
                k,
                "net_kpi_periph",
                "HID / Audio",
                format!(
                    "{} HID · {} audio",
                    s.net_stats.input_events + s.net_stats.gamepad_pkts,
                    s.net_stats.audio_pkts
                ),
                None,
            ))
            .child(if !s.net_stats.zp_path.is_empty() {
                kpi_box(
                    k,
                    "net_kpi_zpsync",
                    "ZPSync P2P",
                    match s.net_stats.zp_rtt_ms {
                        Some(rtt) => format!("{} ({} ms)", s.net_stats.zp_path, rtt),
                        None => s.net_stats.zp_path.clone(),
                    },
                    Some(k.c.text_accent),
                )
            } else {
                rune::el().size(0.0, 0.0)
            }),
    );

    let mut log_col = col().w_full().min_w(0.0).gap(2.0);
    if s.net_logs.is_empty() {
        log_col = log_col.child(
            text(format!("Les logs de {} s'afficheront ici.", bin_name("zcrudp_stream")))
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .wrap_anywhere(),
        );
    } else {
        for (i, line) in s.net_logs.iter().enumerate() {
            log_col = log_col.child(
                text(line.clone())
                    .key(format!("nlog_{i}"))
                    .font(k.ty.mono)
                    .color(k.c.text_base)
                    .wrap_anywhere(),
            );
        }
    }

    col()
        .key("network_view")
        .w_full()
        .min_w(0.0)
        .gap(16.0)
        .child(config_panel)
        .child(periph_panel)
        .child(telemetry)
        .child(panel(
            k,
            "net_logs_panel",
            "Console zcrudp_stream",
            None,
            col()
                .min_h(140.0)
                .max_h(260.0)
                .w_full()
                .min_w(0.0)
                .padding(10.0)
                .radius(k.radius.md)
                .fill(k.c.bg_base)
                .child(log_col.scroll(ScrollSpec { stick_bottom: true }).min_h(0.0)),
        ))
}
