mod controls;
mod pages;

use openscq30_i18n::Translate;
use rune::tok::{self, radius, space, type_};
use rune::{Cx, El, col, el, row, text};
use rune_kit::Kit;

use crate::bt::Cmd;
use crate::state::{AppState, Connection, Page};

pub use controls::Paint;

pub fn card(p: &Paint) -> El {
    col()
        .key("card")
        .padding(space::S1)
        .fill(p.t.card)
        .radius(radius::LG)
        .border(p.hair, p.t.card_border)
}

pub fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = cx.system_dark();
    let t = tok::theme(dark);
    let paint = Paint {
        t,
        kit: Kit::neutral(dark),
        accent: tok::accent(0, dark),
        hair: 1.0 / cx.env().scale,
    };
    let mut layer = Vec::new();
    let content = match state.page {
        Page::Devices => pages::devices(state, &paint, &mut layer),
        Page::Category(category) => pages::category(state, &paint, category, &mut layer),
    };
    let body = row()
        .key("app")
        .w_full()
        .h_full()
        .fill(t.bg)
        .child(sidebar(state, &paint))
        .child(
            el().key("divider")
                .w(1.0)
                .h_full()
                .fill(t.separator)
                .a11y_hidden(),
        )
        .child(content);
    rune_kit::root(body, layer)
}

fn sidebar(state: &AppState, p: &Paint) -> El {
    let model = state
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.model)
        .unwrap_or(state.model);
    let (status, dot) = match state.connection {
        Connection::Connected => ("Connected", p.accent),
        Connection::Connecting => ("Connecting…", rune::oklch(0.78, 0.14, 85.0)),
        Connection::Disconnected => ("Disconnected", p.t.control_off),
    };

    let mut nav = col().key("nav").gap(2.0).mt(space::S3);
    nav = nav.child(nav_row(
        p,
        "devices",
        "Devices",
        state.page == Page::Devices,
        |s: &mut AppState| s.page = Page::Devices,
    ));
    for category in state.categories() {
        let selected = state.page == Page::Category(category);
        nav = nav.child(nav_row(
            p,
            &format!("cat-{category:?}"),
            &category.translate(),
            selected,
            move |s: &mut AppState| s.page = Page::Category(category),
        ));
    }

    let mut footer = col().key("footer").gap(space::S2).px(space::S2);
    if state.is_connected() {
        footer = footer.child(
            rune_kit::Button::new("Disconnect")
                .variant(rune_kit::ButtonVariant::Ghost)
                .control_size(rune_kit::ControlSize::Small)
                .on_press(|s: &mut AppState| s.send(Cmd::Disconnect))
                .build(&p.kit),
        );
    }

    col()
        .key("sidebar")
        .w(236.0)
        .h_full()
        .py(space::S6)
        .px(space::S3)
        .gap(space::S1)
        .fill(p.t.bg)
        .child(
            col()
                .key("brand")
                .gap(2.0)
                .px(space::S2)
                .child(
                    text("Soundcore")
                        .key("brand-title")
                        .font(type_::TITLE)
                        .color(p.t.text),
                )
                .child(
                    row()
                        .key("status")
                        .gap(space::S2)
                        .center_y()
                        .child(el().key("dot").size(7.0, 7.0).radius(3.5).fill(dot))
                        .child(
                            text(status)
                                .key("status-text")
                                .font(type_::LABEL)
                                .color(p.t.text_secondary),
                        ),
                )
                .child(
                    text(model.translate())
                        .key("model")
                        .font(type_::MONO_CAPTION)
                        .color(p.t.text_secondary),
                ),
        )
        .child(nav)
        .child(el().key("spacer").grow(1.0))
        .child(footer)
}

fn nav_row(
    p: &Paint,
    key: &str,
    label: &str,
    selected: bool,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    let mut row = row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .h(32.0)
        .px(space::S3)
        .w_full()
        .center_y()
        .radius(radius::MD)
        .focusable(true)
        .on_press(on_press);
    if selected {
        row = row.fill(p.t.card).border(p.hair, p.t.card_border);
    } else {
        row = row.hover(p.t.hover).press(p.t.press);
    }
    row.child(
        text(label)
            .key("label")
            .font(type_::BODY)
            .color(if selected {
                p.t.text
            } else {
                p.t.text_secondary
            })
            .a11y_hidden(),
    )
}
