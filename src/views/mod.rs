pub mod bench;
pub mod network;
pub mod tests;

use rune::{Color, El, ScrollSpec, TextStyle, col, el, row, spacer, text};
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, ControlSize, Divider, Kit, SegmentedControl,
};

use crate::state::{AppState, Tab, host_platform_badge};

pub fn app_view(s: &AppState, k: &Kit) -> El {
    let tab_idx = match s.tab {
        Tab::Benchmarks => 0,
        Tab::Tests => 1,
        Tab::Network => 2,
    };

    let header = row()
        .key("header")
        .w_full()
        .h(52.0)
        .px(20.0)
        .center_y()
        .gap(16.0)
        .fill(k.c.layer_01)
        .child(
            row()
                .gap(10.0)
                .center_y()
                .child(
                    el().size(10.0, 10.0)
                        .radius(5.0)
                        .fill(if s.bench_running || s.net_stats.running {
                            k.c.success.fg
                        } else {
                            k.c.bg_accent
                        }),
                )
                .child(text("zcstream").font(k.ty.body_medium).color(k.c.text_strong))
                .child(
                    Badge::new("RUDP + Vidéo")
                        .variant(BadgeVariant::Subtle)
                        .build(k),
                )
                .child(
                    Badge::new(host_platform_badge())
                        .variant(BadgeVariant::Neutral)
                        .build(k),
                ),
        )
        .child(spacer())
        .child(
            SegmentedControl::new(&["Pipelines & Latence", "Tests & Validation", "Streaming Réseau"], Some(tab_idx))
                .fit()
                .on_pick(|s: &mut AppState, idx, _| {
                    s.tab = match idx {
                        0 => Tab::Benchmarks,
                        1 => Tab::Tests,
                        _ => Tab::Network,
                    };
                })
                .build(k)
                .key("main_tabs"),
        )
        .child(spacer())
        .child(
            Button::new(if s.dark { "Clair" } else { "Sombre" })
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Regular)
                .on_press(|s: &mut AppState| s.dark = !s.dark)
                .build(k)
                .key("theme_toggle"),
        );

    let body = match s.tab {
        Tab::Benchmarks => bench::view(s, k),
        Tab::Tests => tests::view(s, k),
        Tab::Network => network::view(s, k),
    };

    col()
        .key("shell")
        .w_full()
        .h_full()
        .fill(k.c.page)
        .child(header)
        .child(Divider::horizontal().build(k))
        .child(
            col()
                .key("body_scroll_wrap")
                .grow(1.0)
                .shrink()
                .min_h(0.0)
                .w_full()
                .child(
                    body.w_full()
                        .padding(20.0)
                        .scroll(ScrollSpec { stick_bottom: false }),
                ),
        )
}

pub fn panel(k: &Kit, key: &'static str, title: &str, subtitle: Option<&str>, content: El) -> El {
    let mut header = col().gap(2.0).child(
        text(title.to_owned())
            .font(k.ty.body_medium)
            .color(k.c.text_strong),
    );
    if let Some(sub) = subtitle {
        header = header.child(
            text(sub.to_owned())
                .font(k.ty.small)
                .color(k.c.text_muted),
        );
    }

    col()
        .key(key)
        .w_full()
        .padding(16.0)
        .gap(12.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .child(header)
        .child(Divider::horizontal().build(k))
        .child(content)
}

pub fn kpi_box(k: &Kit, key: &'static str, label: &str, value: String, accent: Option<Color>) -> El {
    let val_color = accent.unwrap_or(k.c.text_strong);
    col()
        .key(key)
        .grow(1.0)
        .padding(12.0)
        .gap(4.0)
        .radius(k.radius.md)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_weak)
        .child(
            text(label.to_owned())
                .font(k.ty.small)
                .color(k.c.text_muted),
        )
        .child(
            text(value)
                .font(TextStyle {
                    size: 18.0,
                    line_height: 22.0,
                    ..k.ty.body_medium
                })
                .color(val_color),
        )
}

pub fn stepper(
    k: &Kit,
    key: &'static str,
    label: &str,
    value_str: String,
    on_dec: impl Fn(&mut AppState) + 'static,
    on_inc: impl Fn(&mut AppState) + 'static,
) -> El {
    row()
        .key(key)
        .w_full()
        .center_y()
        .gap(8.0)
        .child(
            text(label.to_owned())
                .font(k.ty.ui)
                .color(k.c.text_base)
                .grow(1.0),
        )
        .child(
            Button::new("−")
                .variant(ButtonVariant::Outline)
                .control_size(ControlSize::Small)
                .on_press(on_dec)
                .build(k)
                .key("dec"),
        )
        .child(
            el().min_w(64.0)
                .center()
                .child(text(value_str).font(k.ty.mono).color(k.c.text_strong)),
        )
        .child(
            Button::new("+")
                .variant(ButtonVariant::Outline)
                .control_size(ControlSize::Small)
                .on_press(on_inc)
                .build(k)
                .key("inc"),
        )
}
