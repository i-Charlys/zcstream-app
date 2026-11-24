pub mod network;

use rune::{Color, El, ScrollSpec, TextStyle, col, el, row, text};
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, ControlSize, Divider, Kit};

use crate::state::{AppState, host_platform_badge};

pub fn app_view(s: &AppState, k: &Kit) -> El {
    let header = row()
        .key("header")
        .w_full()
        .min_h(52.0)
        .px(16.0)
        .py(10.0)
        .center_y()
        .justify_between()
        .flex_wrap()
        .gap(10.0)
        .fill(k.c.layer_01)
        .child(
            row()
                .flex_wrap()
                .gap(8.0)
                .center_y()
                .child(
                    el().size(10.0, 10.0)
                        .radius(5.0)
                        .fill(if s.net_stats.running {
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
        .child(
            Button::new(if s.dark { "Clair" } else { "Sombre" })
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Regular)
                .on_press(|s: &mut AppState| s.dark = !s.dark)
                .build(k)
                .key("theme_toggle"),
        );

    let body = network::view(s, k);

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
                .min_w(0.0)
                .w_full()
                .child(
                    col()
                        .w_full()
                        .max_w(1180.0)
                        .self_center()
                        .padding(16.0)
                        .child(body)
                        .scroll(ScrollSpec { stick_bottom: false }),
                ),
        )
}

pub fn panel(k: &Kit, key: &'static str, title: &str, subtitle: Option<&str>, content: El) -> El {
    let mut header = col().w_full().min_w(0.0).gap(2.0).child(
        text(title.to_owned())
            .font(k.ty.body_medium)
            .color(k.c.text_strong)
            .wrap(),
    );
    if let Some(sub) = subtitle {
        header = header.child(
            text(sub.to_owned())
                .font(k.ty.small)
                .color(k.c.text_muted)
                .wrap(),
        );
    }

    col()
        .key(key)
        .w_full()
        .min_w(0.0)
        .shrink()
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
        .shrink()
        .min_w(150.0)
        .padding(12.0)
        .gap(4.0)
        .radius(k.radius.md)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_weak)
        .child(
            text(label.to_owned())
                .font(k.ty.small)
                .color(k.c.text_muted)
                .wrap(),
        )
        .child(
            text(value)
                .font(TextStyle {
                    size: 18.0,
                    line_height: 22.0,
                    ..k.ty.body_medium
                })
                .color(val_color)
                .wrap(),
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
        .flex_wrap()
        .center_y()
        .justify_between()
        .gap(8.0)
        .child(
            text(label.to_owned())
                .font(k.ty.ui)
                .color(k.c.text_base)
                .wrap()
                .min_w(140.0)
                .shrink(),
        )
        .child(
            row()
                .center_y()
                .gap(8.0)
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
                ),
        )
}
