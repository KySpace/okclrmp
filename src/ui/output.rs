use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use okclrmp::preview::{self, CmykProof, Fallback, SamplingPreview};

impl StudioPanel {
    fn set_parameter(&mut self, value: f64, cx: &mut Context<Self>) {
        if !value.is_finite() {
            return;
        }
        self.model.update(cx, |m, cx| {
            let value = value.clamp(0., 1.);
            if m.project.cursor != value {
                m.project.cursor = value;
                cx.notify();
            }
        });
    }
    fn scrub_bar(&mut self, position: Pixels, cx: &mut Context<Self>) {
        if let Some(t) = preview::parameter_from_position(
            f32::from(position) as f64,
            f32::from(self.bar_bounds.left()) as f64,
            f32::from(self.bar_bounds.size.width) as f64,
        ) {
            self.set_bar_fraction(t, cx);
        }
    }
    fn set_bar_fraction(&mut self, fraction: f64, cx: &mut Context<Self>) {
        if !fraction.is_finite() {
            return;
        }
        let p = &self.model.read(cx).project;
        let value = self
            .sampling_preview
            .parameter(fraction, p.sample_count, p.reverse_output);
        self.set_parameter(value, cx);
    }
    fn snap_bar_selection(&mut self, cx: &mut Context<Self>) {
        let p = &self.model.read(cx).project;
        if self.sampling_preview.count(p.sample_count).is_some() {
            let position =
                self.sampling_preview
                    .position(p.cursor, p.sample_count, p.reverse_output);
            self.set_bar_fraction(position, cx);
        }
    }
    fn step_bar(&mut self, direction: f64, fast: bool, cx: &mut Context<Self>) {
        let p = &self.model.read(cx).project;
        let current = preview::output_position(p.cursor, p.reverse_output);
        let next = if let Some(n) = self.sampling_preview.count(p.sample_count) {
            ((current * (n - 1) as f64).round() + direction) / (n - 1) as f64
        } else {
            current + direction * if fast { 0.01 } else { 0.001 }
        };
        self.set_bar_fraction(next, cx);
    }
    fn copy_output(
        &self,
        text: &str,
        success: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = match crate::clipboard::copy_text(window, cx, text) {
            Ok(()) => success,
            Err(e) => format!("Copy failed: {e}"),
        };
        self.model.update(cx, |m, cx| {
            m.notice = message;
            cx.notify();
        });
    }
    pub(super) fn output(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.proof.is_none() {
            self.proof = Some(CmykProof::new());
        }
        let proof_error = self.proof.as_ref().unwrap().as_ref().err().cloned();
        let colors = palette(cx);
        let m = self.model.read(cx);
        let p = m.project.clone();
        let invalid = m.error.is_some();
        let count = p.sample_count;
        let data = exchange::export_ordered(
            &m.curve,
            count,
            self.export_space,
            p.map_chroma,
            p.reverse_output,
        );
        let oog = m
            .curve
            .sample(count)
            .iter()
            .filter(|s| !s.color.in_gamut())
            .count();
        let current = m.curve.at(p.cursor);
        let [r, g, b, _] = current.rgba8(p.map_chroma);
        let swatch = rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32);
        let cursor = p.cursor;
        let position = self
            .sampling_preview
            .position(cursor, count, p.reverse_output);
        let ordered_cursor = preview::output_position(cursor, p.reverse_output);
        let step = self
            .sampling_preview
            .count(count)
            .map_or(0.001, |n| 1. / (n - 1) as f64);
        let param = p.parameter.clone();
        let mut raw = m
            .curve
            .sample(self.sampling_preview.count(count).unwrap_or(512));
        if p.reverse_output {
            raw.reverse();
        }
        let reverse = p.reverse_output;
        let source: Vec<_> = raw.iter().map(|s| s.color.srgb(p.map_chroma)).collect();
        let display = match self.fallback {
            Fallback::Original => source.clone(),
            Fallback::Grayscale => source.iter().copied().map(preview::grayscale).collect(),
            Fallback::Cmyk => {
                match self
                    .proof
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|proof| proof.convert(&source))
                {
                    Ok(rgb) => rgb,
                    Err(error) => {
                        self.fallback_error = Some(format!("CMYK preview unavailable: {error}"));
                        self.fallback = Fallback::Original;
                        source.clone()
                    }
                }
            }
        };
        let pixels: Vec<_> = display
            .iter()
            .zip(&raw)
            .map(|(rgb, s)| {
                (
                    rgb.map(|v| (v.clamp(0., 1.) * 255.).round() as u8),
                    !s.color.in_gamut(),
                )
            })
            .collect();
        let state = self.output_text.get_or_insert_with(|| {
            cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(12)
                    .soft_wrap(true)
                    .default_value(data.clone())
            })
        });
        if state.read(cx).value().as_ref() != data.as_str() {
            state.update(cx, |s, cx| s.set_value(data.clone(), window, cx));
        }
        let text = state.clone();
        let dragging = self.bar_drag;
        let weak = cx.weak_entity();
        let paint_weak = weak.clone();
        let bar = canvas(
            move |bounds, _, cx| {
                let _ = weak.update(cx, |this, _| this.bar_bounds = bounds);
                bounds
            },
            move |_, bounds, window, cx| {
                for (i, (rgb_value, outside)) in pixels.iter().enumerate() {
                    let x = bounds.left() + bounds.size.width * (i as f32 / pixels.len() as f32);
                    let width = bounds.size.width / pixels.len() as f32 + px(0.5);
                    let [r, g, b] = *rgb_value;
                    window.paint_quad(fill(
                        Bounds::new(point(x, bounds.top()), size(width, bounds.size.height)),
                        rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32),
                    ));
                    if *outside {
                        window.paint_quad(fill(
                            Bounds::new(point(x, bounds.bottom() - px(3.)), size(width, px(3.))),
                            rgb(colors.warning),
                        ));
                    }
                }
                let x = bounds.left() + bounds.size.width * position as f32;
                for (width, color) in [(4., colors.curve_outline), (2., colors.curve)] {
                    line(
                        point(x, bounds.top()),
                        point(x, bounds.bottom()),
                        width,
                        color,
                        window,
                    );
                }
                outlined_label(
                    format!("{param} = {cursor:.3}"),
                    point(
                        (x + px(5.)).clamp(
                            bounds.left() + px(4.),
                            (bounds.right() - px(100.)).max(bounds.left() + px(4.)),
                        ),
                        bounds.top() + px(4.),
                    ),
                    window,
                    cx,
                );
                for (label, x) in [
                    (if reverse { "1.0" } else { "0.0" }, bounds.left() + px(4.)),
                    (
                        if reverse { "0.0" } else { "1.0" },
                        bounds.right() - px(26.),
                    ),
                ] {
                    outlined_label(
                        label.into(),
                        point(x, bounds.bottom() - px(18.)),
                        window,
                        cx,
                    );
                }
                if dragging {
                    let moving = paint_weak.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            let _ = moving.update(cx, |this, cx| {
                                if this.bar_drag && event.pressed_button == Some(MouseButton::Left)
                                {
                                    this.scrub_bar(event.position.x, cx);
                                } else {
                                    this.bar_drag = false;
                                    cx.notify();
                                }
                            });
                        }
                    });
                    let released = paint_weak.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                            let _ = released.update(cx, |this, cx| {
                                this.bar_drag = false;
                                cx.notify();
                            });
                        }
                    });
                }
            },
        )
        .size_full();
        let increment = cx.weak_entity();
        let decrement = cx.weak_entity();
        let set_value = cx.weak_entity();
        let bar = div()
            .id("colormap-scrubber")
            .h(px(50.))
            .w_full()
            .cursor_col_resize()
            .overflow_hidden()
            .track_focus(&self.focus)
            .role(Role::Slider)
            .aria_label("Inspect colormap position")
            .aria_numeric_value(ordered_cursor)
            .aria_min_numeric_value(0.)
            .aria_max_numeric_value(1.)
            .aria_numeric_value_step(step)
            .aria_orientation(Orientation::Horizontal)
            .border_1()
            .border_color(rgb(if self.focus.is_focused(window) {
                colors.accent
            } else {
                colors.border
            }))
            .tooltip(|window, cx| {
                Tooltip::new("Click or drag to inspect · Left/Right: step · Home/End: endpoints")
                    .build(window, cx)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    this.bar_drag = true;
                    this.scrub_bar(event.position.x, cx);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.bar_drag && event.pressed_button == Some(MouseButton::Left) {
                    this.scrub_bar(event.position.x, cx);
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if !this.focus.is_focused(window) {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "left" => this.step_bar(-1., event.keystroke.modifiers.shift, cx),
                    "right" => this.step_bar(1., event.keystroke.modifiers.shift, cx),
                    "home" => this.set_bar_fraction(0., cx),
                    "end" => this.set_bar_fraction(1., cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .on_a11y_action(AccessibleAction::Increment, move |_, _, cx| {
                let _ = increment.update(cx, |this, cx| {
                    this.step_bar(1., false, cx);
                });
            })
            .on_a11y_action(AccessibleAction::Decrement, move |_, _, cx| {
                let _ = decrement.update(cx, |this, cx| {
                    this.step_bar(-1., false, cx);
                });
            })
            .on_a11y_action(AccessibleAction::SetValue, move |data, _, cx| {
                if let Some(gpui_kit::accesskit::ActionData::NumericValue(value)) = data {
                    let _ = set_value.update(cx, |this, cx| this.set_bar_fraction(*value, cx));
                }
            })
            .child(bar);
        let weak = cx.weak_entity();
        let mode = self.fallback;
        let fallback_tip = proof_error.clone().map_or_else(
            || {
                format!(
                    "Fallback: {}. CMYK uses SWOP; grayscale preserves luminance. Preview only.",
                    mode.label()
                )
            },
            |e| format!("CMYK is unavailable: {e}. Grayscale and original previews are available"),
        );
        let fallback = Button::new("fallback-colors")
            .label(format!("{} ▾", mode.short_label()))
            .small()
            .ghost()
            .flex_shrink_0()
            .accessibility_label(format!("Fallback preview: {}", mode.label()))
            .tooltip(fallback_tip)
            .dropdown_menu(move |mut menu, _, _| {
                for option in Fallback::ALL {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(option.label())
                            .checked(mode == option)
                            .disabled(option == Fallback::Cmyk && proof_error.is_some())
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |s, cx| {
                                    s.fallback = option;
                                    s.fallback_error = None;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        let weak = cx.weak_entity();
        let selected_preview = self.sampling_preview;
        let preview_menu = Button::new("sampling-preview")
            .label(format!("{} ▾",selected_preview.label(count))).small().ghost().flex_shrink_0()
            .accessibility_label(format!("Colorbar sampling preview: {}",selected_preview.label(count)))
            .tooltip("Colorbar sampling preview · choose continuous or discrete swatches, independently of export count")
            .dropdown_menu(move |mut menu,_,_| {
                for option in [SamplingPreview::Continuous,SamplingPreview::Output].into_iter()
                    .chain(SamplingPreview::COUNTS.into_iter().map(SamplingPreview::Samples)) {
                    let weak=weak.clone();
                    let label=if option==SamplingPreview::Output {format!("Use output count ({count})")} else {option.label(count)};
                    menu=menu.item(PopupMenuItem::new(label).checked(selected_preview==option)
                        .on_click(move |_,_,cx| {let _=weak.update(cx,|s,cx| {
                            s.sampling_preview=option; s.snap_bar_selection(cx); cx.notify();
                        });}));
                }
                menu
            });
        let preview_controls=row().gap_1().w_full()
            .child(fallback)
            .child(tool_button("mapping",include_bytes!("../icons/clip.svg"),"Clip RGB channels",
                if p.map_chroma {"Chroma mapping active · click to clip RGB channels. Coral underline marks the original path outside sRGB"}
                else {"RGB clipping active · click to map chroma into sRGB. Coral underline marks the original path outside sRGB"})
                .selected(!p.map_chroma)
                .on_click(cx.listener(|this,_,_,cx| this.model.update(cx,|m,cx| {
                    m.project.map_chroma = !m.project.map_chroma; cx.notify();
                }))))
            .child(preview_menu)
            .child(tool_button("invert-colorbar",include_bytes!("../icons/invert.svg"),"Invert colorbar and output",
                "Reverse the colorbar and exported sample order. The inspected source color stays selected")
                .selected(p.reverse_output)
                .on_click(cx.listener(|this,_,_,cx| this.model.update(cx,|m,cx| {
                    m.project.reverse_output = !m.project.reverse_output; cx.notify();
                }))))
            .child(div().flex_1())
            .child(caption(format!("{oog} outside sRGB"),cx).min_w_0().truncate());
        let weak = cx.weak_entity();
        let output_count = Button::new("output-count")
            .label(format!("Samples: {count} ▾"))
            .small()
            .ghost()
            .flex_shrink_0()
            .tooltip("Number of serialized output samples")
            .dropdown_menu(move |mut menu, _, _| {
                for n in SamplingPreview::COUNTS {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(n.to_string())
                            .checked(count == n)
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |s, cx| {
                                    s.model.update(cx, |m, cx| {
                                        m.project.sample_count = n;
                                        cx.notify();
                                    });
                                    s.snap_bar_selection(cx);
                                });
                            }),
                    );
                }
                menu
            });
        let weak = cx.weak_entity();
        let encoding = self.export_space;
        let encoding_menu = Button::new("output-encoding")
            .label(format!("{} ▾", encoding.label()))
            .small()
            .ghost()
            .flex_shrink_0()
            .tooltip("Output encoding")
            .dropdown_menu(move |mut menu, _, _| {
                for space in ExportSpace::ALL {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(space.label())
                            .checked(encoding == space)
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |s, cx| {
                                    s.export_space = space;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        let copy = data.clone();
        let output_controls = row()
            .gap_1()
            .w_full()
            .child(output_count)
            .child(encoding_menu)
            .child(div().flex_1())
            .child(
                tool_button(
                    "copy-colors",
                    include_bytes!("../icons/copy.svg"),
                    "Copy sampled colors",
                    "Copy all serialized sampled colors",
                )
                .disabled(invalid)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.copy_output(&copy, "Colormap copied to clipboard".into(), window, cx);
                })),
            )
            .child(
                tool_button(
                    "save-colors",
                    include_bytes!("../icons/export.svg"),
                    "Export sampled colors",
                    "Export all sampled colors to a file",
                )
                .disabled(invalid)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.save_text(
                        data.clone(),
                        if matches!(this.export_space, ExportSpace::Hex | ExportSpace::Rgb) {
                            "colormap.json"
                        } else {
                            "colormap.csv"
                        },
                        cx,
                    );
                })),
            );
        let hex = current.hex(p.map_chroma);
        let color_button = Button::new("copy-inspected-color")
            .small()
            .ghost()
            .px_1()
            .flex_shrink_0()
            .accessibility_label(format!("Copy {hex} as HEX"))
            .tooltip("Copy the inspected color as HEX")
            .child(
                row()
                    .gap_1()
                    .child(div().w_7().h_5().rounded_sm().bg(swatch))
                    .child(caption(hex.clone(), cx).font_family("Consolas")),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.copy_output(&hex, format!("Copied {hex}"), window, cx)
            }));
        let inspector = row()
            .gap_2()
            .w_full()
            .child(color_button)
            .child(
                caption(
                    format!(
                        "L {:.3}  C {:.3}  H {:.1}°",
                        current.l, current.c, current.h
                    ),
                    cx,
                )
                .font_family("Consolas")
                .flex_shrink_0(),
            )
            .child(
                tool_button(
                    "follow",
                    include_bytes!("../icons/move-slices.svg"),
                    "Move slices to this color",
                    "Move unbound slices to the inspected color",
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.model.update(cx, |m, cx| m.sync_slices(window, cx))
                })),
            )
            .child(div().flex_1());
        let mut content = column()
            .w_full()
            .p_3()
            .gap_2()
            .bg(rgb(colors.surface))
            .child(
                div()
                    .id("preview-controls")
                    .w_full()
                    .overflow_x_scroll()
                    .child(preview_controls),
            )
            .child(bar)
            .child(
                div()
                    .id("inspector-controls")
                    .w_full()
                    .overflow_x_scroll()
                    .child(inspector),
            )
            .child(
                div()
                    .id("output-controls")
                    .w_full()
                    .overflow_x_scroll()
                    .child(output_controls),
            )
            .child(
                Textarea::new(&text)
                    .readonly(true)
                    .h(px(240.))
                    .w_full()
                    .font_family("Consolas")
                    .aria_label("All serialized sampled colors"),
            );
        if let Some(error) = &self.fallback_error {
            content = content.child(caption(error.clone(), cx).text_color(rgb(colors.warning)));
        }
        div()
            .id("output-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(colors.surface))
            .child(content)
            .into_any_element()
    }
}
