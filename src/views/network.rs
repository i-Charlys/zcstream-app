use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rune::{El, ScrollSpec, TextEdit, col, row, spacer, text};
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, ControlSize, Divider, Kit, SegmentedControl,
    Spinner, Switch, TextInput,
};

use crate::net_udp::{ZpSyncLaunchOpts, probe_zpsync_network, start_network_session};
use crate::state::{AppState, NetConnectMode, NetRole, RESOLUTIONS, StreamCodec};
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
        StreamCodec::Ternary => 1,
    };

    let config_panel = panel(
        k,
        "net_cfg_panel",
        "Streaming Écran Temps Réel (zcrudp_stream.exe)",
        Some("Capture DXGI Desktop Duplication + encodage GPU + transport zcrudp (2 PC ou loopback)"),
        col()
            .w_full()
            .gap(12.0)
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(12.0)
                    .child(text("Rôle").font(k.ty.ui).color(k.c.text_base).w(110.0))
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
                    )
                    .child(spacer())
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
                    .center_y()
                    .gap(12.0)
                    .child(text("Codec GPU").font(k.ty.ui).color(k.c.text_base).w(110.0))
                    .child(
                        SegmentedControl::new(&["NVENC (H.264)", "Ternary HLSL"], Some(codec_idx))
                            .fit()
                            .on_pick(|s: &mut AppState, idx, _| {
                                if !s.net_stats.running {
                                    s.net_codec = if idx == 0 {
                                        StreamCodec::Nvenc
                                    } else {
                                        StreamCodec::Ternary
                                    };
                                }
                            })
                            .build(k)
                            .key("net_codec_seg"),
                    )
                    .child(text("Résolution").font(k.ty.ui).color(k.c.text_base).ml(16.0))
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
            )
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(12.0)
                    .child(text("Transport").font(k.ty.ui).color(k.c.text_base).w(110.0))
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
                    )
                    .child(spacer())
                    .child(if s.net_connect_mode == NetConnectMode::ZpSync {
                        row()
                            .center_y()
                            .gap(8.0)
                            .child(if let Some(ref info) = s.zp_probe_info {
                                text(info.clone()).font(k.ty.small).color(k.c.text_muted)
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
                    .center_y()
                    .gap(12.0)
                    .child(
                        text(if s.net_role == NetRole::Sender {
                            "IP Cible"
                        } else {
                            "Écoute sur"
                        })
                        .font(k.ty.ui)
                        .color(k.c.text_base)
                        .w(110.0),
                    )
                    .child(if s.net_role == NetRole::Sender {
                        TextInput::new(&s.net_host_edit)
                            .placeholder("192.168.1.50")
                            .width(Some(220.0))
                            .on_edit(|s: &mut AppState| &mut s.net_host_edit)
                            .build(k)
                            .key("net_host_input")
                    } else {
                        text("0.0.0.0 (toutes interfaces)")
                            .font(k.ty.mono)
                            .color(k.c.text_muted)
                    })
                    .child(text("Port UDP").font(k.ty.ui).color(k.c.text_base).ml(16.0))
                    .child(
                        TextInput::new(&s.net_port_edit)
                            .placeholder("9900")
                            .width(Some(100.0))
                            .on_edit(|s: &mut AppState| &mut s.net_port_edit)
                            .build(k)
                            .key("net_port_input"),
                    )
            } else {
                col()
                    .w_full()
                    .gap(8.0)
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(12.0)
                            .child(text("Secret (S)").font(k.ty.ui).color(k.c.text_base).w(110.0))
                            .child(
                                TextInput::new(&s.zp_secret_edit)
                                    .placeholder("zcstream-p2p")
                                    .width(Some(200.0))
                                    .on_edit(|s: &mut AppState| &mut s.zp_secret_edit)
                                    .build(k)
                                    .key("zp_secret_input"),
                            )
                            .child(text("IP Pair (opt.)").font(k.ty.ui).color(k.c.text_base).ml(8.0))
                            .child(
                                TextInput::new(&s.zp_peer_ip_edit)
                                    .placeholder("Auto (DHT)")
                                    .width(Some(160.0))
                                    .on_edit(|s: &mut AppState| &mut s.zp_peer_ip_edit)
                                    .build(k)
                                    .key("zp_peer_ip_input"),
                            )
                            .child(text("Port pair").font(k.ty.ui).color(k.c.text_base).ml(4.0))
                            .child(
                                TextInput::new(&s.zp_peer_port_edit)
                                    .placeholder("0")
                                    .width(Some(68.0))
                                    .on_edit(|s: &mut AppState| &mut s.zp_peer_port_edit)
                                    .build(k)
                                    .key("zp_peer_port_input"),
                            )
                            .child(text("Local").font(k.ty.ui).color(k.c.text_base).ml(4.0))
                            .child(
                                TextInput::new(&s.zp_local_port_edit)
                                    .placeholder("0")
                                    .width(Some(68.0))
                                    .on_edit(|s: &mut AppState| &mut s.zp_local_port_edit)
                                    .build(k)
                                    .key("zp_local_port_input"),
                            ),
                    )
                    .child(
                        row()
                            .w_full()
                            .center_y()
                            .gap(20.0)
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
                    .gap(8.0)
                    .child(stepper(
                        k,
                        "net_fps",
                        "Cadence cible",
                        format!("{} fps", s.net_fps),
                        |s| s.net_fps = s.net_fps.saturating_sub(15).max(15),
                        |s| s.net_fps = (s.net_fps + 15).min(240),
                    ))
                    .child(stepper(
                        k,
                        "net_mbps",
                        "Débit cible",
                        format!("{} Mbps", s.net_bitrate_mbps),
                        |s| s.net_bitrate_mbps = s.net_bitrate_mbps.saturating_sub(5).max(5),
                        |s| s.net_bitrate_mbps = (s.net_bitrate_mbps + 10).min(400),
                    ))
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
                    .child(
                        Switch::new(s.net_use_rio)
                            .label("Winsock RIO (Registered I/O)")
                            .description("Envoi UDP zéro-syscall sur l'émetteur")
                            .on_toggle(|s: &mut AppState| s.net_use_rio = !s.net_use_rio)
                            .build(k)
                            .key("net_sw_rio"),
                    )
            } else {
                text("Le récepteur ouvre une fenêtre FLIP_DISCARD temps réel dès réception des frames.")
                    .font(k.ty.small)
                    .color(k.c.text_muted)
            })
            .child(
                row()
                    .w_full()
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
                        .on_press(|s: &mut AppState| {
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
                            s.net_stats.status_msg = "Lancement de zcrudp_stream.exe...".into();
                            start_network_session(
                                s.net_role,
                                s.net_connect_mode,
                                s.net_codec,
                                width,
                                height,
                                host,
                                port,
                                zp_opts,
                                s.net_fps,
                                s.net_bitrate_mbps,
                                s.net_fec_shards,
                                s.net_use_rio,
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
                        text(s.net_stats.status_msg.clone())
                            .font(k.ty.ui)
                            .color(k.c.text_muted),
                    ),
            ),
    );

    let telemetry = panel(
        k,
        "net_telemetry",
        "Télémétrie Temps Réel",
        None,
        row()
            .w_full()
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

    let mut log_col = col().w_full().gap(2.0);
    if s.net_logs.is_empty() {
        log_col = log_col.child(
            text("Les logs de zcrudp_stream.exe s'afficheront ici.")
                .font(k.ty.mono)
                .color(k.c.text_muted),
        );
    } else {
        for (i, line) in s.net_logs.iter().enumerate() {
            log_col = log_col.child(
                text(line.clone())
                    .key(format!("nlog_{i}"))
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            );
        }
    }

    col()
        .key("network_view")
        .w_full()
        .gap(16.0)
        .child(config_panel)
        .child(telemetry)
        .child(panel(
            k,
            "net_logs_panel",
            "Console zcrudp_stream",
            None,
            col()
                .h(160.0)
                .w_full()
                .padding(10.0)
                .radius(k.radius.md)
                .fill(k.c.bg_base)
                .child(log_col.scroll(ScrollSpec { stick_bottom: true }).min_h(0.0)),
        ))
}
