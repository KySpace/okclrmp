use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

fn ramp_thumbnail(colors: Arc<[u32]>, width: f32) -> impl IntoElement {
    div()
        .w(px(width))
        .h(px(14.))
        .flex_shrink_0()
        .rounded_sm()
        .overflow_hidden()
        .child(
            canvas(
                |bounds, _, _| bounds,
                move |_, bounds, window, _| {
                    for (i, color) in colors.iter().enumerate() {
                        let x =
                            bounds.left() + bounds.size.width * (i as f32 / colors.len() as f32);
                        window.paint_quad(fill(
                            Bounds::new(
                                point(x, bounds.top()),
                                size(
                                    bounds.size.width / colors.len() as f32 + px(0.5),
                                    bounds.size.height,
                                ),
                            ),
                            rgb(*color),
                        ));
                    }
                },
            )
            .size_full(),
        )
}
fn preview_colors(curve: &Curve, map: bool) -> Arc<[u32]> {
    curve
        .sample(64)
        .iter()
        .map(|s| {
            let [r, g, b, _] = s.color.rgba8(map);
            ((r as u32) << 16) | ((g as u32) << 8) | b as u32
        })
        .collect::<Vec<_>>()
        .into()
}
impl StudioPanel {
    pub(super) fn starting_point_picker(&self, cx: &Context<Self>) -> impl IntoElement {
        let m = self.model.read(cx);
        let colors = preview_colors(&m.curve, m.project.map_chroma);
        let mapping = m.project.map_chroma;
        let weak = cx.weak_entity();
        Button::new("starting-points")
            .small()
            .w_full()
            .accessibility_label("Choose a starting colormap")
            .tooltip("Built-in curves from the sample projects")
            .child(
                row()
                    .w_full()
                    .gap_2()
                    .child(div().flex_1().child("Starting point"))
                    .child(ramp_thumbnail(colors, 80.))
                    .child("▾"),
            )
            .dropdown_menu(move |mut menu, _, _| {
                let mut group = "";
                for preset in StartingPoint::ALL {
                    if preset.group() != group {
                        group = preset.group();
                        menu = menu.item(PopupMenuItem::label(group));
                    }
                    let project = preset.project();
                    let colors =
                        preview_colors(&project.compile().expect("valid starting point"), mapping);
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, cx| {
                            row()
                                .w_full()
                                .min_w(px(300.))
                                .justify_between()
                                .gap_3()
                                .child(caption(preset.name(), cx).text_color(rgb(palette(cx).text)))
                                .child(ramp_thumbnail(colors.clone(), 96.))
                        })
                        .on_click(move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.model.update(cx, |m, cx| {
                                    let mut p = m.project.clone();
                                    preset.apply(&mut p);
                                    match p.compile() {
                                        Ok(curve) => {
                                            p.follow_slices(&curve);
                                            if let Err(error) = m.set_project(p, window, cx) {
                                                m.notice = error;
                                                cx.notify();
                                            }
                                        }
                                        Err(error) => {
                                            m.notice = error;
                                            cx.notify();
                                        }
                                    }
                                });
                            });
                        }),
                    );
                }
                menu
            })
    }
}
