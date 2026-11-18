use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use okclrmp::constants::{Constant, ConstantKind, limit_precision, step_number_with_binding};

struct LabelDrag {
    origin: Pixels,
    value: f64,
    moved: bool,
}

/// Each row owns its text/caret state; the shared model only stores valid scalars.
pub(super) struct ConstantEditor {
    owner: WeakEntity<StudioModel>,
    name: Entity<InputState>,
    value: Entity<InputState>,
    kind: ConstantKind,
    binding: Option<SliderBinding>,
    renaming: bool,
    original_name: String,
    drag: Option<LabelDrag>,
    scroll: f32,
    _subscriptions: Vec<Subscription>,
}
impl ConstantEditor {
    pub(super) fn new(
        constant: &Constant,
        owner: WeakEntity<StudioModel>,
        renaming: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).default_value(constant.name.clone()));
        let value = cx.new(|cx| {
            InputState::new(window, cx).default_value(constant.kind.display(constant.value))
        });
        let mut subscriptions = vec![
            cx.subscribe_in(&name, window, |this, _, event, window, cx| {
                match event {
                    InputEvent::Change => this.publish(window, cx),
                    InputEvent::Blur => this.renaming = false,
                    InputEvent::PressEnter { .. } => {
                        this.renaming = false;
                        this.value.update(cx, |s, cx| s.focus(window, cx));
                    }
                    _ => {}
                }
                cx.notify();
            }),
            cx.subscribe_in(&value, window, |this, _, event, window, cx| match event {
                InputEvent::Change => {
                    this.limit_precision(window, cx);
                    this.publish(window, cx);
                }
                InputEvent::Blur | InputEvent::PressEnter { .. } => {
                    this.scroll = 0.;
                    this.commit_value(window, cx);
                }
                _ => {}
            }),
        ];
        if let Some(model) = owner.upgrade() {
            subscriptions.push(cx.observe(&model, |_, _, cx| cx.notify()));
        }
        Self {
            owner,
            name,
            value,
            kind: constant.kind,
            binding: constant.binding,
            renaming,
            original_name: constant.name.clone(),
            drag: None,
            scroll: 0.,
            _subscriptions: subscriptions,
        }
    }
    pub(super) fn focus_name(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name.update(cx, |s, cx| {
            s.focus(window, cx);
            s.set_selected_range(0..s.value().len(), cx);
        });
    }
    pub(super) fn sync_value(&mut self, value: f64, window: &mut Window, cx: &mut Context<Self>) {
        self.drag = None;
        self.scroll = 0.;
        self.value.update(cx, |s, cx| {
            s.set_value(self.kind.display(value), window, cx)
        });
        cx.notify();
    }
    fn choose_binding(
        &mut self,
        binding: Option<SliderBinding>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot(cx).is_err() {
            self.publish(window, cx);
            return;
        }
        self.commit_value(window, cx);
        let id = cx.entity().entity_id();
        if let Some(owner) = self.owner.upgrade() {
            let constant = owner.update(cx, |m, cx| {
                let index = m.constants.iter().position(|r| r.entity_id() == id)?;
                match m.project.bind_constant(index, binding) {
                    Ok(()) => {
                        m.sync_slider_states(window, cx);
                        m.rebuild(cx);
                        Some(m.project.constants[index].clone())
                    }
                    Err(error) => {
                        m.notice = error;
                        cx.notify();
                        None
                    }
                }
            });
            if let Some(constant) = constant {
                self.binding = constant.binding;
                self.sync_value(constant.value, window, cx);
            }
        }
    }
    fn snapshot(&self, cx: &App) -> Result<Constant, String> {
        let name = self.name.read(cx).value().to_string();
        let value = self
            .value
            .read(cx)
            .value()
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(|v| self.kind.constrain(v))
            .ok_or_else(|| format!("Constant ‘{name}’ needs a finite number"))?;
        let mut constant = Constant {
            name,
            value,
            kind: self.kind,
            binding: self.binding,
        };
        constant.value = constant.constrain(value).unwrap();
        Ok(constant)
    }
    fn publish(&self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.snapshot(cx);
        let id = cx.entity().entity_id();
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |m, cx| {
                if let Some(index) = m.constants.iter().position(|r| r.entity_id() == id) {
                    match result {
                        Ok(constant) => match m.project.update_constant(index, constant) {
                            Ok(()) => {
                                m.constant_errors.remove(&id);
                                m.sync_slider_states(window, cx);
                            }
                            Err(error) => {
                                m.constant_errors.insert(id, error);
                            }
                        },
                        Err(error) => {
                            m.constant_errors.insert(id, error);
                        }
                    }
                    m.rebuild(cx);
                }
            });
        }
    }
    fn commit_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.limit_precision(window, cx);
        if let Ok(constant) = self.snapshot(cx) {
            // Keep intentional trailing zeroes while the value is in range.
            let text = self.value.read(cx).value();
            if text.parse::<f64>().ok() != Some(constant.value) {
                self.value.update(cx, |s, cx| {
                    s.set_value(self.kind.display(constant.value), window, cx)
                });
            }
        }
        self.publish(window, cx);
    }
    fn limit_precision(&self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.value.read(cx);
        let text = state.value();
        if let Some(rounded) = limit_precision(&text, self.kind) {
            let cursor = state.cursor().min(rounded.len());
            self.value.update(cx, |s, cx| {
                s.set_value(rounded, window, cx);
                s.set_selected_range(cursor..cursor, cx);
            });
        }
    }
    fn step(
        &mut self,
        direction: i32,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let state = self.value.read(cx);
        if !state.focus_handle(cx).is_focused(window) || !state.selected_range().is_empty() {
            return false;
        }
        let mut text = state.value().to_string();
        let mut cursor = state.cursor();
        for _ in 0..count {
            let Some(edit) =
                step_number_with_binding(&text, cursor, self.kind, self.binding, direction)
            else {
                return false;
            };
            text = edit.text;
            cursor = edit.cursor;
        }
        self.value.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.set_selected_range(cursor..cursor, cx);
        });
        self.publish(window, cx);
        true
    }
    fn drag_to(&mut self, position: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let distance = f64::from(f32::from(position - drag.origin));
        if !drag.moved && distance.abs() < 2. {
            return;
        }
        drag.moved = true;
        if let Some(value) = self.kind.drag(drag.value, distance) {
            let value = self.binding.map_or(value, |b| value.clamp(0., b.max()));
            self.value.update(cx, |s, cx| {
                s.set_value(self.kind.display(value), window, cx)
            });
            self.publish(window, cx);
        }
    }
}
impl Render for ConstantEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = palette(cx);
        let weak = cx.entity().downgrade();
        let binding_weak = weak.clone();
        let current_kind = self.kind;
        let binding = self.binding;
        let owners: [Option<String>; 3] = std::array::from_fn(|axis| {
            let owner = self.owner.upgrade()?;
            let model = owner.read(cx);
            let index = model.project.bound_constant(axis)?;
            (model.constants[index].entity_id() != cx.entity().entity_id())
                .then(|| model.project.constants[index].name.clone())
        });
        let label = if self.renaming {
            div()
                .flex_1()
                .min_w_0()
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.name.update(cx, |s, cx| {
                            s.set_value(this.original_name.clone(), window, cx)
                        });
                        this.renaming = false;
                        this.value.update(cx, |s, cx| s.focus(window, cx));
                        this.publish(window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                }))
                .child(Input::new(&self.name).small().aria_label("Constant name"))
                .into_any_element()
        } else {
            div()
                .id("constant-name")
                .flex_1()
                .min_w_0()
                .h(px(28.))
                .flex()
                .items_center()
                .cursor_col_resize()
                .rounded_sm()
                .px_1()
                .hover(|s| s.bg(rgb(colors.border)))
                .text_sm()
                .font_family("Consolas")
                .tooltip(|window, cx| {
                    Tooltip::new("Drag left/right to tune · double-click to rename")
                        .build(window, cx)
                })
                .child(div().truncate().child(self.name.read(cx).value()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if event.click_count == 2 {
                            this.drag = None;
                            this.original_name = this.name.read(cx).value().to_string();
                            this.renaming = true;
                            this.focus_name(window, cx);
                        } else if let Ok(constant) = this.snapshot(cx) {
                            this.drag = Some(LabelDrag {
                                origin: event.position.x,
                                value: constant.value,
                                moved: false,
                            });
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .into_any_element()
        };
        row()
            .relative()
            .gap_1()
            .w_full()
            .child(label)
            .child(
                div()
                    .id("constant-number")
                    .flex_1()
                    .min_w_0()
                    .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        let key = &event.keystroke;
                        if key.modifiers.control
                            || key.modifiers.alt
                            || key.modifiers.platform
                            || key.modifiers.shift
                        {
                            return;
                        }
                        let direction = match key.key.as_str() {
                            "up" => 1,
                            "down" => -1,
                            _ => return,
                        };
                        if this.step(direction, 1, window, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .on_scroll(cx.listener(|this, event: &ScrollEvent, window, cx| {
                        let state = this.value.read(cx);
                        if !state.focus_handle(cx).is_focused(window)
                            || !state.selected_range().is_empty()
                            || step_number_with_binding(&state.value(), state.cursor(), this.kind, this.binding, 1).is_none()
                        {
                            return;
                        }
                        this.scroll += f32::from(event.delta.pixel_delta(px(20.)).y) / 20.;
                        let steps = this.scroll.trunc().clamp(-100., 100.) as i32;
                        if steps != 0 {
                            this.scroll -= steps as f32;
                            this.step(steps.signum(), steps.unsigned_abs() as usize, window, cx);
                        }
                        cx.stop_propagation();
                    }))
                    .child(
                        Input::new(&self.value)
                            .small()
                            .font_family("Consolas")
                            .aria_label(format!("{} value", self.name.read(cx).value())),
                    ),
            )
            .child(
                Button::new("constant-kind")
                    .label(current_kind.name())
                    .small()
                    .ghost()
                    .w(px(62.))
                    .p_0()
                    .tooltip(current_kind.description())
                    .dropdown_menu(move |mut menu, _, _| {
                        for kind in ConstantKind::ALL {
                            let weak = weak.clone();
                            menu = menu.item(
                                PopupMenuItem::new(kind.name())
                                    .checked(kind == current_kind)
                                    .on_click(move |_, window, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            this.kind = kind;
                                            if this.binding.is_some_and(|b| !kind.can_bind(b)) {
                                                this.binding = None;
                                            }
                                            this.commit_value(window, cx);
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            )
            .child(
                tool_button("constant-binding", include_bytes!("../icons/link-slider.svg"),
                    "Bind constant to slice slider",
                    if let Some(binding) = binding {
                        format!("Bound to {} · edits work in both directions and share the spline constant. Click to change or unlink", binding.name())
                    } else if matches!(current_kind, ConstantKind::Integer | ConstantKind::Free) {
                        "Choose Degree to bind H, or Fraction to bind L/C".into()
                    } else {
                        "Bind to a slice slider · Degree → H, Fraction → L/C. C uses 0–0.4 directly".into()
                    })
                    .selected(binding.is_some())
                    .disabled(matches!(current_kind, ConstantKind::Integer | ConstantKind::Free))
                    .dropdown_menu(move |mut menu, _, _| {
                        let weak = binding_weak.clone();
                        menu = menu.item(PopupMenuItem::new("Unbound").checked(binding.is_none())
                            .on_click(move |_, window, cx| { let _ = weak.update(cx, |s, cx| s.choose_binding(None, window, cx)); }));
                        for target in SliderBinding::ALL {
                            let weak = binding_weak.clone();
                            let owner = &owners[target.axis()];
                            let label = owner.as_ref().map_or_else(|| target.label().to_string(), |name| format!("{} · used by {name}", target.name()));
                            menu = menu.item(PopupMenuItem::new(label).checked(binding == Some(target))
                                .disabled(!current_kind.can_bind(target) || owner.is_some())
                                .on_click(move |_, window, cx| { let _ = weak.update(cx, |s, cx| s.choose_binding(Some(target), window, cx)); }));
                        }
                        menu
                    })
            )
            .child(
                Button::new("remove-constant")
                    .label("×")
                    .small()
                    .ghost()
                    .w(px(20.))
                    .p_0()
                    .accessibility_label("Remove constant")
                    .tooltip("Remove this constant")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let id = cx.entity().entity_id();
                        if let Some(owner) = this.owner.upgrade() {
                            owner.update(cx, |m, cx| {
                                if let Some(index) =
                                    m.constants.iter().position(|r| r.entity_id() == id)
                                {
                                    m.constants.remove(index);
                                    m.project.constants.remove(index);
                                    m.constant_errors.remove(&id);
                                    m.rebuild(cx);
                                }
                            });
                        }
                    })),
            )
            .when(self.drag.is_some(), |r| {
                // Register while dragging so the gesture continues outside the
                // label, including outside the docked editor's bounds.
                let weak = cx.entity().downgrade();
                r.child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let moving = weak.clone();
                            window.on_mouse_event(
                                move |event: &MouseMoveEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Bubble {
                                        let _ = moving.update(cx, |this, cx| {
                                            if event.pressed_button == Some(MouseButton::Left) {
                                                this.drag_to(event.position.x, window, cx);
                                            } else {
                                                this.drag = None;
                                                cx.notify();
                                            }
                                        });
                                    }
                                },
                            );
                            let released = weak.clone();
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble
                                    && event.button == MouseButton::Left
                                {
                                    let _ = released.update(cx, |this, cx| {
                                        this.drag = None;
                                        cx.notify();
                                    });
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
    }
}
