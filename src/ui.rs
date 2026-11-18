use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, Theme, ThemeMode,
    button::{Button, ButtonVariants},
    dock::{self, BasePanel, DockArea, DockLayout, DockPlacement, DockSkin, Panel, PanelEvent},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    slider::{Slider, SliderEvent, SliderState},
    tooltip::Tooltip,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use okclrmp::{
    appearance::{Appearance, Palette},
    color::Oklch,
    exchange::{self, ExportSpace, ImportSpace},
    gpu::{self, Gpu},
    model::{self, CURVE_SAMPLES, Curve, Mode, Plane, Project, Sample},
    spline::{self, Drag, NodeMode, Side, Spline},
};
use std::{rc::Rc, sync::Arc, time::Duration};
#[path = "ui/constant_editor.rs"]
mod constant_editor;
#[path = "ui/output.rs"]
mod output;
use constant_editor::ConstantEditor;
use okclrmp::constants::{Constant, ConstantKind, SliderBinding};

const PLOT_HELP_DELAY: Duration = Duration::from_secs(2);
fn tool_button(
    id: impl Into<ElementId>,
    svg: &[u8],
    name: impl Into<SharedString>,
    tip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .icon(Icon::default().data(svg))
        .small()
        .ghost()
        .w(px(24.))
        .h(px(24.))
        .p_0()
        .flex_shrink_0()
        .accessibility_label(name)
        .tooltip(tip)
}

fn appearance(cx: &App) -> Appearance {
    if cx.theme().is_dark() {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}
fn palette(cx: &App) -> Palette {
    appearance(cx).palette()
}
fn column() -> Div {
    div().flex().flex_col().gap_3()
}
fn row() -> Div {
    div().flex().items_center().gap_2()
}
fn caption(s: impl Into<SharedString>, cx: &App) -> Div {
    let colors = palette(cx);
    div()
        .text_xs()
        .text_color(rgb(colors.muted))
        .child(s.into())
}
fn section(s: &str, cx: &App) -> Div {
    let colors = palette(cx);
    div()
        .mt_2()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(colors.accent))
        .child(s.to_string())
}
fn color_value(c: Oklch) -> Rgba {
    let a = c.rgba8(true);
    rgb(((a[0] as u32) << 16) | ((a[1] as u32) << 8) | a[2] as u32)
}

pub struct StudioModel {
    pub project: Project,
    pub curve: Curve,
    pub error: Option<String>,
    pub notice: String,
    pub gpu: Arc<Gpu>,
    parameter: Entity<InputState>,
    constants: Vec<Entity<ConstantEditor>>,
    constant_errors: std::collections::HashMap<EntityId, String>,
    definitions: Entity<TextareaState>,
    channels: [Entity<InputState>; 3],
    sliders: [Entity<SliderState>; 3],
    import_text: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}
impl StudioModel {
    pub fn new(gpu: Arc<Gpu>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = Project::default();
        let curve = project.compile().unwrap();
        let parameter =
            cx.new(|cx| InputState::new(window, cx).default_value(project.parameter.clone()));
        let constants = project
            .constants
            .iter()
            .map(|constant| {
                let owner = cx.entity().downgrade();
                cx.new(|cx| ConstantEditor::new(constant, owner, false, window, cx))
            })
            .collect();
        let definitions = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .placeholder("bias = 2 * abs(t - 0.5)")
                .default_value(project.definitions.clone())
        });
        let channels = std::array::from_fn(|i| {
            cx.new(|cx| InputState::new(window, cx).default_value(project.expressions[i].clone()))
        });
        let sliders = std::array::from_fn(|i| {
            cx.new(|_| {
                SliderState::new()
                    .min(0.)
                    .max([1., 0.4, 360.][i])
                    .step([0.001, 0.001, 0.1][i])
                    .default_value(project.slices[i] as f32)
            })
        });
        let import_text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(8)
                .default_value("#440154\n#414487\n#2a788e\n#22a884\n#7ad151\n#fde725")
        });
        let mut subscriptions = vec![];
        for input in [&parameter, &channels[0], &channels[1], &channels[2]] {
            subscriptions.push(cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.read_inputs(cx);
                }
            }));
        }
        subscriptions.push(
            cx.subscribe(&definitions, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.read_inputs(cx);
                }
            }),
        );
        for (i, slider) in sliders.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                slider,
                window,
                move |this, _, event: &SliderEvent, window, cx| {
                    if let SliderEvent::Change(value) = event {
                        // Programmatic slider synchronization must not round
                        // a saved f64 constant to f32 unless the user moves it.
                        if this.project.slices[i] as f32 == value.start() {
                            return;
                        }
                        let mut changed = this.project.set_slice(
                            i,
                            (value.start() as f64).clamp(0., [1., model::C_MAX, 360.][i]),
                        );
                        if let Some(index) = this.project.bound_constant(i) {
                            let row = &this.constants[index];
                            let constant = &this.project.constants[index];
                            changed |= this.constant_errors.remove(&row.entity_id()).is_some();
                            row.update(cx, |s, cx| s.sync_value(constant.value, window, cx));
                        }
                        if changed {
                            this.rebuild(cx);
                            return;
                        }
                        cx.notify();
                    }
                },
            ));
        }
        Self {
            project,
            curve,
            error: None,
            notice: "Ready".into(),
            gpu,
            parameter,
            constants,
            constant_errors: std::collections::HashMap::new(),
            definitions,
            channels,
            sliders,
            import_text,
            _subscriptions: subscriptions,
        }
    }
    fn read_inputs(&mut self, cx: &mut Context<Self>) {
        self.project.parameter = self.parameter.read(cx).value().to_string();
        self.project.definitions = self.definitions.read(cx).value().to_string();
        self.project.expressions =
            std::array::from_fn(|i| self.channels[i].read(cx).value().to_string());
        self.rebuild(cx);
    }
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        if let Some(error) = self.constant_errors.values().next() {
            self.error = Some(error.clone());
            cx.notify();
            return;
        }
        match self.project.compile() {
            Ok(curve) => {
                self.curve = curve;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        };
        cx.notify();
    }
    fn set_project(
        &mut self,
        mut p: Project,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        p.migrate_spline()?;
        p.sync_bound_slices()?;
        let curve = p.compile()?;
        self.project = p;
        self.curve = curve;
        self.error = None;
        self.parameter.update(cx, |s, cx| {
            s.set_value(self.project.parameter.clone(), window, cx)
        });
        self.constants = self
            .project
            .constants
            .iter()
            .map(|constant| {
                let owner = cx.entity().downgrade();
                cx.new(|cx| ConstantEditor::new(constant, owner, false, window, cx))
            })
            .collect();
        self.constant_errors.clear();
        self.definitions.update(cx, |s, cx| {
            s.set_value(self.project.definitions.clone(), window, cx)
        });
        for i in 0..3 {
            self.channels[i].update(cx, |s, cx| {
                s.set_value(self.project.expressions[i].clone(), window, cx)
            });
        }
        for i in 0..3 {
            let v = self.project.slices[i];
            self.sliders[i].update(cx, |s, cx| s.set_value(v as f32, window, cx));
        }
        cx.notify();
        Ok(())
    }
    fn add_constant(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name =
            (1..)
                .map(|i| format!("k{i}"))
                .find(|name| {
                    name != &self.project.parameter
                        && self.project.constants.iter().all(|c| c.name != *name)
                        && self.project.definitions.split([';', '\n']).all(|entry| {
                            entry.split_once('=').is_none_or(|(n, _)| n.trim() != name)
                        })
                })
                .unwrap();
        let constant = Constant {
            name,
            value: 0.,
            kind: ConstantKind::Free,
            binding: None,
        };
        let owner = cx.entity().downgrade();
        let row = cx.new(|cx| ConstantEditor::new(&constant, owner, true, window, cx));
        row.update(cx, |s, cx| s.focus_name(window, cx));
        self.constants.push(row);
        self.project.constants.push(constant);
        self.rebuild(cx);
    }
    fn sync_slices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.project.follow_slices(&self.curve);
        self.sync_slider_states(window, cx);
        cx.notify();
    }
    fn sync_slider_states(&self, window: &mut Window, cx: &mut Context<Self>) {
        for i in 0..3 {
            self.sliders[i].update(cx, |s, cx| {
                s.set_value(self.project.slices[i] as f32, window, cx)
            });
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Editor,
    Plot(Option<Plane>),
    Output,
    Import,
}
impl Kind {
    fn title(self) -> &'static str {
        match self {
            Self::Editor => "Curve studio",
            Self::Plot(Some(p)) => p.name(),
            Self::Plot(None) => "Gamut volume",
            Self::Output => "Colormap output",
            Self::Import => "Reference colors",
        }
    }
}

struct StudioPanel {
    kind: Kind,
    model: Entity<StudioModel>,
    focus: FocusHandle,
    _observer: Subscription,
    bounds: Bounds<Pixels>,
    cache: Option<(Vec<u64>, Arc<RenderImage>)>,
    spline_drag: Option<Drag>,
    selected_node: Option<usize>,
    orbit_start: Option<Point<Pixels>>,
    hue_pan_start: Option<Point<Pixels>>,
    help_position: Option<Point<Pixels>>,
    help_timer: Option<Task<()>>,
    help_tooltip: Option<AnyTooltip>,
    export_space: ExportSpace,
    import_space: ImportSpace,
    bar_bounds: Bounds<Pixels>,
    bar_drag: bool,
    fallback: okclrmp::preview::Fallback,
    sampling_preview: okclrmp::preview::SamplingPreview,
    proof: Option<Result<okclrmp::preview::CmykProof, String>>,
    fallback_error: Option<String>,
    output_text: Option<Entity<TextareaState>>,
}
impl StudioPanel {
    fn new(kind: Kind, model: Entity<StudioModel>, cx: &mut Context<Self>) -> Self {
        let observer = cx.observe(&model, |_, _, cx| cx.notify());
        Self {
            kind,
            model,
            focus: cx.focus_handle(),
            _observer: observer,
            bounds: Bounds::default(),
            cache: None,
            spline_drag: None,
            selected_node: None,
            orbit_start: None,
            hue_pan_start: None,
            help_position: None,
            help_timer: None,
            help_tooltip: None,
            export_space: ExportSpace::Hex,
            import_space: ImportSpace::Hex,
            bar_bounds: Bounds::default(),
            bar_drag: false,
            fallback: Default::default(),
            sampling_preview: Default::default(),
            proof: None,
            fallback_error: None,
            output_text: None,
        }
    }
    fn editor(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let m = self.model.read(cx);
        let p = m.project.clone();
        let mut content = column()
            .p_4()
            .gap_3()
            .child(caption("DESIGN A PATH THROUGH COLOR", cx))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(p.name.clone()),
            )
            .child(section("STARTING POINTS", cx));
        for (i, name) in [
            "01  Monochrome",
            "02  Linear sweep",
            "03  Diverging",
            "04  Turbo study",
        ]
        .iter()
        .enumerate()
        {
            content = content.child(
                Button::new(("preset", i))
                    .label(*name)
                    .w_full()
                    .small()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.model.update(cx, |m, cx| {
                            let _ = m.set_project(Project::preset(i), window, cx);
                        });
                    })),
            );
        }
        let expressions = matches!(p.mode, Mode::Expressions);
        content = content
            .child(section("CURVE DEFINITION", cx))
            .child(
                row()
                    .child(
                        Button::new("expr-mode")
                            .label("Expressions")
                            .small()
                            .selected(expressions)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |m, cx| {
                                    m.project.use_expressions();
                                    m.rebuild(cx);
                                })
                            })),
                    )
                    .child(caption(
                        match p.mode {
                            Mode::Spline(plane) => format!("{} spline", plane.short()),
                            Mode::Imported => "Imported path".into(),
                            _ => "L · C · H".into(),
                        },
                        cx,
                    )),
            )
            .child(
                row().child(caption("Parameter", cx).w_24()).child(
                    Input::new(&m.parameter)
                        .small()
                        .aria_label("Parameter name"),
                ),
            )
            .child(caption("One parameter on [0, 1]. Hue is in degrees.", cx))
            .child(
                row()
                    .justify_between()
                    .child(caption("Constants", cx))
                    .child(
                        Button::new("add-constant")
                            .label("+ Add")
                            .small()
                            .ghost()
                            .tooltip("Add a typed constant · double-click a name to rename")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.model.update(cx, |m, cx| m.add_constant(window, cx))
                            })),
                    ),
            );
        for constant in &m.constants {
            content = content.child(constant.clone());
        }
        content = content
            .child(caption("Variables · name = expression", cx))
            .child(
                Textarea::new(&m.definitions)
                    .small()
                    .font_family("Consolas")
                    .aria_label("Variable definitions"),
            )
            .child(caption(
                "Use the parameter, constants, and earlier variables.",
                cx,
            ));
        for i in 0..3 {
            let constant = p.constant_axis() == Some(i);
            let locked = constant
                || match p.mode {
                    Mode::Expressions => false,
                    Mode::Spline(plane) => i != plane.axes().2,
                    Mode::Imported => true,
                };
            content = content.child(
                column()
                    .gap_1()
                    .child(
                        row()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(colors.channels[i]))
                                    .child(format!("{}({})", model::axis_name(i), p.parameter)),
                            )
                            .when(locked, |r| {
                                r.child(caption(
                                    if constant {
                                        format!(
                                            "constant {} · slider",
                                            (if i == 2 {
                                                ConstantKind::Degree
                                            } else {
                                                ConstantKind::Fraction
                                            })
                                            .display(p.slices[i])
                                        )
                                    } else if matches!(p.mode, Mode::Imported) {
                                        "from reference".into()
                                    } else {
                                        "from spline".into()
                                    },
                                    cx,
                                ))
                            }),
                    )
                    .child(
                        Input::new(&m.channels[i])
                            .small()
                            .font_family("Consolas")
                            .disabled(locked)
                            .aria_label(format!("{} expression", model::axis_name(i))),
                    ),
            );
        }
        if let Some(error) = &m.error {
            content = content
                .child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(colors.error_background))
                        .text_xs()
                        .text_color(rgb(colors.warning))
                        .child(error.clone()),
                )
                .child(caption(
                    "Plots show the last valid curve. Export is paused.",
                    cx,
                ));
        }
        if let Mode::Spline(plane) = p.mode
            && let Some(s) = &p.spline
        {
            content = content.child(section("SPLINE NODES", cx));
            for (i, n) in s.nodes.iter().enumerate() {
                let v = plane.from_point(n.position);
                content = content.child(
                    caption(
                        format!(
                            "{} · {}={:.3}  ({:.4}, {:.4})",
                            i + 1,
                            p.parameter,
                            s.parameter(i),
                            v[0],
                            v[1]
                        ),
                        cx,
                    )
                    .font_family("Consolas"),
                );
            }
        }
        content=content.child(section("EXPRESSION NOTES", cx))
            .child(caption("+, −, *, /, ^ · comparisons · && · ||\ncondition ? a : b\nabs, sin, cos, sind, sqrt, min, max, clamp, mod, pi\nUse explicit * for multiplication.", cx));
        div()
            .id("editor-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(colors.surface))
            .child(content)
            .into_any_element()
    }
    fn edit_spline(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Spline)) {
        self.spline_drag = None;
        self.model.update(cx, |m, cx| {
            if matches!(m.project.mode, Mode::Spline(_))
                && let Some(spline) = &mut m.project.spline
            {
                edit(spline);
                m.rebuild(cx);
            }
        });
    }
    fn spline_tools(&self, s: Option<&Spline>, cx: &Context<Self>) -> Div {
        let active = s.is_some();
        let linear = s.is_none_or(|s| s.linear);
        let middle = s.is_some_and(|s| s.nodes.len() == 3);
        let symmetric = s.is_some_and(|s| s.symmetric);
        let toggles = row()
            .gap_0()
            .flex_shrink_0()
            .child(
                tool_button(
                    "curve-kind",
                    include_bytes!("icons/cubic.svg"),
                    "Cubic curve",
                    if linear {
                        "Use a cubic Bézier curve with editable handles"
                    } else {
                        "Use a straight line with two anchors and no handles"
                    },
                )
                .selected(!linear)
                .disabled(!active)
                .on_click(
                    cx.listener(|this, _, _, cx| this.edit_spline(cx, |s| s.set_linear(!s.linear))),
                ),
            )
            .child(
                tool_button(
                    "middle-node",
                    include_bytes!("icons/middle-node.svg"),
                    "Middle node",
                    if linear {
                        "Middle nodes require cubic mode"
                    } else if middle {
                        "Remove the middle node; preserve endpoint tangents"
                    } else {
                        "Add a middle node without changing the curve's shape"
                    },
                )
                .selected(middle)
                .disabled(linear)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.edit_spline(cx, |s| {
                        if s.nodes.len() == 3 {
                            s.remove_middle()
                        } else {
                            s.add_middle()
                        }
                    })
                })),
            )
            .child(
                tool_button(
                    "symmetry",
                    include_bytes!("icons/symmetry.svg"),
                    "Symmetric editing",
                    "Mirror endpoint heights and handles about their vertical midpoint",
                )
                .selected(symmetric)
                .disabled(!active)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.edit_spline(cx, |s| s.set_symmetry(!s.symmetric))
                })),
            )
            .when(!active, |el| el.invisible());
        // Keep the middle-node tools' slot even when they are hidden.
        let mut modes = row()
            .gap_0()
            .w(px(101.))
            .flex_shrink_0()
            .border_l_1()
            .border_color(rgb(palette(cx).border))
            .pl(px(4.));
        for (i, mode, icon, name, tip) in [
            (
                0usize,
                NodeMode::Corner,
                include_bytes!("icons/corner.svg").as_slice(),
                "Corner node",
                "Corner: free the tangents on the first click; remove middle handles on the second",
            ),
            (
                1,
                NodeMode::Smooth,
                include_bytes!("icons/smooth.svg").as_slice(),
                "Smooth node",
                "Smooth: align to C1; dragging keeps the opposite handle's length",
            ),
            (
                2,
                NodeMode::Auto,
                include_bytes!("icons/auto-smooth.svg").as_slice(),
                "Auto C2 node",
                "Auto-smooth: solve C2 now; manual edits retain C1",
            ),
            (
                3,
                NodeMode::Symmetric,
                include_bytes!("icons/symmetric-smooth.svg").as_slice(),
                "Symmetric smooth node",
                "Symmetric-smooth: keep C1 and equal handle lengths",
            ),
        ] {
            modes = modes.child(
                tool_button(("middle-mode", i), icon, name, tip)
                    .selected(s.is_some_and(|s| middle && s.nodes[1].mode == mode))
                    .disabled(!middle)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_spline(cx, |s| s.set_middle_mode(mode))
                    })),
            );
        }
        row()
            .gap_1()
            .flex_shrink_0()
            .child(toggles)
            .child(modes.when(!middle, |el| el.invisible()))
    }
    fn toggle_spline(&mut self, plane: Plane, cx: &mut Context<Self>) {
        self.spline_drag = None;
        self.model.update(cx, |m, cx| {
            match m.project.mode {
                Mode::Expressions => m.project.enable_spline(plane, &m.curve),
                Mode::Spline(active) if active == plane => m.project.use_expressions(),
                _ => return,
            }
            m.rebuild(cx);
        });
    }
    fn toggle_constant(&mut self, plane: Plane, cx: &mut Context<Self>) {
        self.model.update(cx, |m, cx| {
            if m.project.mode == Mode::Spline(plane) {
                m.project.spline_constant = !m.project.spline_constant;
                m.rebuild(cx);
            }
        });
    }
    fn save_text(&mut self, text: String, name: &str, cx: &mut Context<Self>) {
        let prompt =
            cx.prompt_for_new_path(&std::env::current_dir().unwrap_or_default(), Some(name));
        cx.spawn(async move |this, cx| match prompt.await {
            Ok(Ok(Some(path))) => {
                let message = match std::fs::write(&path, text) {
                    Ok(_) => format!("Saved {}", path.display()),
                    Err(e) => format!("Could not save: {e}"),
                };
                let _ = this.update(cx, |this, cx| {
                    this.model.update(cx, |m, cx| {
                        m.notice = message;
                        cx.notify();
                    })
                });
            }
            Ok(Err(e)) => {
                let _ = this.update(cx, |this, cx| {
                    this.model.update(cx, |m, cx| {
                        m.notice = format!("File dialog failed: {e}");
                        cx.notify();
                    })
                });
            }
            _ => {}
        })
        .detach();
    }
    fn importer(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let m = self.model.read(cx);
        let imported = m.project.imported.clone();
        let mut spaces = row();
        for (i, space) in [ImportSpace::Hex, ImportSpace::Rgb, ImportSpace::Oklch]
            .into_iter()
            .enumerate()
        {
            spaces = spaces.child(
                Button::new(("import-space", i))
                    .label(space.label())
                    .small()
                    .selected(self.import_space == space)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.import_space = space;
                        cx.notify();
                    })),
            );
        }
        let mut content=column().p_4().child(caption("COMPARE AN EXISTING COLORMAP", cx))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Reference colors"))
            .child(spaces)
            .child(caption("Paste hex colors, a JSON array of triples, or three CSV columns. OKLCH uses L 0–1, C, and H in degrees.", cx))
            .child(Textarea::new(&m.import_text).h(px(180.)).font_family("Consolas").aria_label("Reference colors to import"))
            .child(row().child(Button::new("plot-reference").label("Plot colors").small().primary().on_click(cx.listener(|this,_,_,cx|this.import_colors(cx))))
                .child(Button::new("read-reference").label("Open file…").small().on_click(cx.listener(|this,_,window,cx|this.open_reference(window,cx)))))
            .child(caption(format!("{} reference points",imported.len()), cx))
            .child(Button::new("use-reference").label("Use as colormap path").small().disabled(imported.len()<2).on_click(cx.listener(|this,_,_,cx|this.model.update(cx,|m,cx|{m.project.mode=Mode::Imported;m.rebuild(cx);m.notice="Reference path uses piecewise linear OKLCH interpolation with shortest hue arcs.".into();}))))
            .child(Button::new("clear-reference").label("Clear reference overlay").small().disabled(imported.is_empty()).on_click(cx.listener(|this,_,_,cx|this.model.update(cx,|m,cx|{
                if m.project.mode==Mode::Imported{m.project.mode=Mode::Expressions;}m.project.imported.clear();m.rebuild(cx);
            }))))
            .child(section("PROJECTION DEPTH", cx))
            .child(caption("Points near the slice are small and sharp. Distant points become larger, softer circles. All imported points remain in OKLCH.", cx));
        for (i, c) in imported.iter().take(32).enumerate() {
            content = content.child(
                row()
                    .child(div().w_5().h_5().rounded_sm().bg(color_value(*c)))
                    .child(
                        caption(
                            format!("{:02}  L {:.3}  C {:.3}  H {:.1}", i + 1, c.l, c.c, c.h),
                            cx,
                        )
                        .font_family("Consolas"),
                    ),
            );
        }
        div()
            .id("import-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(colors.surface))
            .child(content)
            .into_any_element()
    }
    fn import_colors(&mut self, cx: &mut Context<Self>) {
        let source = self.model.read(cx).import_text.read(cx).value().to_string();
        let space = self.import_space;
        self.model.update(cx, |m, cx| {
            match exchange::parse(&source, space) {
                Ok(colors) => {
                    m.notice = format!("Imported {} colors", colors.len());
                    m.project.imported = colors;
                    if m.project.mode == Mode::Imported {
                        m.rebuild(cx);
                    }
                }
                Err(e) => m.notice = format!("Import: {e}"),
            };
            cx.notify();
        });
    }
    fn open_reference(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open colormap".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await
                && let Some(path) = paths.first()
            {
                let source = std::fs::read_to_string(path);
                let _ = this.update_in(cx, |this, window, cx| match source {
                    Ok(source) => {
                        this.model
                            .read(cx)
                            .import_text
                            .clone()
                            .update(cx, |s, cx| s.set_value(source, window, cx));
                        this.import_colors(cx);
                    }
                    Err(e) => this.model.update(cx, |m, cx| {
                        m.notice = format!("Could not read file: {e}");
                        cx.notify();
                    }),
                });
            }
        })
        .detach();
    }
    fn plot(&mut self, plane: Option<Plane>, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let m = self.model.read(cx);
        let p = &m.project;
        let mut header = row().gap_1().h(px(24.)).flex_shrink_0();
        if let Some(plane) = plane {
            let active = p.mode == Mode::Spline(plane);
            let locked =
                matches!(p.mode,Mode::Spline(other)if other!=plane) || p.mode == Mode::Imported;
            let tip=match p.mode {
                Mode::Expressions=>format!("Edit a spline in {}; the orthogonal {} channel remains an expression",plane.short(),model::axis_name(plane.axes().2)),
                Mode::Spline(owner) if owner==plane=>"Quit spline editing and return to the saved L, C, and H expressions".into(),
                Mode::Spline(owner)=>format!("Locked: {} owns the spline channels. Quit spline editing there to edit this plane",owner.short()),
                Mode::Imported=>"Locked: reference colors define this path. Switch to Expressions to edit a spline".into(),
            };
            header = header
                .child(
                    div()
                        .id("spline-tool-scroll")
                        .flex_1()
                        .min_w_0()
                        .overflow_x_scroll()
                        .when(active, |el| {
                            el.child(self.spline_tools(p.spline.as_ref(), cx))
                        }),
                )
                .when(active, |el| {
                    let axis = plane.axes().2;
                    let name = model::axis_name(axis);
                    let tip = if p.spline_constant {
                        format!("{name} = {} for the whole colormap. The {name} slice slider changes this constant. Click to restore its saved expression",(if axis == 2 { ConstantKind::Degree } else { ConstantKind::Fraction }).display(p.slices[axis]))
                    } else {
                        format!("Hold {name} constant at this plane's slice value and bind the {name} slider to the colormap")
                    };
                    el.child(
                        tool_button("spline-constant", include_bytes!("icons/constant.svg"), format!("Constant {name}"), tip)
                            .selected(p.spline_constant)
                            .on_click(cx.listener(move |this, _, _, cx| this.toggle_constant(plane, cx))),
                    )
                })
                .child(
                    tool_button(
                        "spline-edit",
                        if locked {
                            include_bytes!("icons/locked.svg")
                        } else {
                            include_bytes!("icons/edit-curve.svg")
                        },
                        if locked {
                            "Spline editing locked"
                        } else if active {
                            "Quit spline editing"
                        } else {
                            "Edit spline"
                        },
                        tip,
                    )
                    .selected(active)
                    .disabled(locked)
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_spline(plane, cx))),
                );
        } else {
            let controls = row().gap_0().flex_shrink_0()
                .child(
                    tool_button("polar-view", include_bytes!("icons/polar-view.svg"), "Polar gamut",
                        if p.polar { "Polar coordinates: C radius, H azimuth, L up. Click for rectangular coordinates" }
                        else { "Rectangular coordinates: H-L floor, C height. Click for polar coordinates" })
                        .selected(p.polar)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.project.set_polar(!m.project.polar);
                                cx.notify();
                            })
                        })),
                )
                .child(
                    tool_button("slice-contours", include_bytes!("icons/slice-contours.svg"), "Slice contours",
                        "Toggle surface slice contours: L in blue, C in gold, H in purple")
                        .selected(p.show_slice_contours)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.project.show_slice_contours = !m.project.show_slice_contours;
                                cx.notify();
                            })
                        })),
                )
                .child(
                    tool_button("orthographic", include_bytes!("icons/orthographic.svg"), "Orthographic projection",
                        if p.orthographic { "Orthographic projection with parallel rays. Click for perspective" }
                        else { "Perspective projection. Click for orthographic projection with parallel rays" })
                        .selected(p.orthographic)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.project.orthographic = !m.project.orthographic;
                                cx.notify();
                            })
                        })),
                )
                .child(
                    tool_button("reset-camera", include_bytes!("icons/reset-view.svg"), "Reset view",
                        "Reset camera orientation, zoom, and hue origin")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.project.reset_view();
                                cx.notify();
                            })
                        })),
                );
            header = header
                .child(
                    caption(
                        if p.polar {
                            "C radius · H azimuth · L up"
                        } else {
                            "H × L floor · C height"
                        },
                        cx,
                    )
                    .flex_1()
                    .min_w_0()
                    .truncate(),
                )
                .child(controls);
        }
        let weak = cx.weak_entity();
        let canvas = canvas(
            move |bounds, window, cx| {
                weak.update(cx, |this, cx| {
                    this.bounds = bounds;
                    if let Some(tooltip) = &this.help_tooltip {
                        window.set_tooltip(tooltip.clone());
                    }
                    let m = this.model.read(cx);
                    let p = m.project.clone();
                    let [w, h] = m.gpu.image_size(
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                        window.scale_factor(),
                    );
                    let appearance = appearance(cx);
                    let mut key = vec![w as u64, h as u64, appearance as u64];
                    if let Some(plane) = plane {
                        key.push(p.slices[plane.axes().2].to_bits());
                        if plane.axes().0 == 2 {
                            key.push(p.hue_origin.to_bits());
                        }
                    } else {
                        key.extend([
                            p.yaw.to_bits() as u64,
                            p.pitch.to_bits() as u64,
                            p.zoom.to_bits() as u64,
                            p.hue_origin.to_bits(),
                            p.polar as u64,
                            p.orthographic as u64,
                            p.show_slice_contours as u64,
                        ]);
                        if p.show_slice_contours {
                            key.extend(p.slices.map(f64::to_bits));
                        }
                    }
                    if this.cache.as_ref().is_none_or(|(k, _)| *k != key) {
                        match m.gpu.render_themed(plane, &p, w, h, appearance) {
                            Ok(mut pixels) => {
                                for rgba in pixels.as_chunks_mut::<4>().0 {
                                    rgba.swap(0, 2);
                                }
                                let image = Arc::new(RenderImage::new(vec![image::Frame::new(
                                    image::RgbaImage::from_raw(w, h, pixels).unwrap(),
                                )]));
                                if let Some((_, old)) = this.cache.replace((key, image)) {
                                    let _ = window.drop_image(old);
                                }
                            }
                            Err(e) => eprintln!("Plot render failed: {e}"),
                        }
                    }
                    (
                        this.cache.as_ref().map(|(_, i)| i.clone()),
                        p,
                        m.curve.clone(),
                        this.selected_node,
                    )
                })
                .ok()
            },
            move |bounds, prepared, window, cx| {
                if let Some((image, p, curve, selected)) = prepared {
                    if let Some(image) = image {
                        let _ = window.paint_image(
                            bounds,
                            bounds,
                            Corners::all(px(0.)),
                            image,
                            0,
                            false,
                        );
                    }
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        paint_plot(bounds, plane, &p, &curve, selected, window, cx)
                    });
                }
            },
        )
        .size_full();
        let mut bottom = column().gap_1();
        if let Some(plane) = plane {
            let (_, _, axis) = plane.axes();
            let bound = p.bound_constant(axis).map(|i| &p.constants[i]);
            let spline_constant = p.constant_axis() == Some(axis);
            let mut slider_label = format!("{} slice", model::axis_name(axis));
            if let Some(constant) = bound {
                slider_label.push_str(&format!("; linked to constant ‘{}’", constant.name));
            }
            if spline_constant {
                slider_label.push_str("; also controls the spline's orthogonal constant");
            }
            let slider_tip = slider_label.clone();
            let intersections = m.curve.intersections(plane, p.slices[axis]);
            let text = if intersections.coplanar {
                "Entire curve lies in this slice".into()
            } else if intersections.points.is_empty() {
                "No intersections with this slice".into()
            } else {
                format!(
                    "Intersections: {}",
                    intersections
                        .points
                        .iter()
                        .take(6)
                        .map(|s| format!("{}={:.3}", p.parameter, s.t))
                        .collect::<Vec<_>>()
                        .join(" · ")
                )
            };
            bottom =
                bottom
                    .child(
                        row()
                            .h(px(24.))
                            .child(div().flex_1().min_w_0().child(
                                Slider::new(&m.sliders[axis]).accessibility_label(slider_label),
                            ))
                            .child(
                                caption(
                                    format!(
                                        "{} {}{}",
                                        model::axis_name(axis),
                                        (if axis == 2 {
                                            ConstantKind::Degree
                                        } else {
                                            ConstantKind::Fraction
                                        })
                                        .display(p.slices[axis]),
                                        if axis == 2 { "°" } else { "" }
                                    ),
                                    cx,
                                )
                                .w(px(88.))
                                .flex_shrink_0()
                                .font_family("Consolas")
                                .text_color(rgb(if spline_constant || bound.is_some() {
                                    colors.accent
                                } else {
                                    colors.muted
                                }))
                                .text_right()
                                .id(("slice-readout", axis))
                                .tooltip(move |window, cx| {
                                    Tooltip::new(slider_tip.clone()).build(window, cx)
                                }),
                            ),
                    )
                    .child(caption(text, cx).truncate());
        }
        let content = column()
            .size_full()
            .min_h_0()
            .p_2()
            .gap_1()
            .bg(rgb(colors.surface))
            .child(header);
        content
            .child(
                div()
                    .id("plot-canvas")
                    .relative()
                    .flex_1()
                    .min_h(px(80.))
                    .w_full()
                    .overflow_hidden()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(colors.border))
                    .child(canvas)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.mouse_down(event.position, window, cx)
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.dismiss_help(cx);
                            this.mouse_move(event.position, event.modifiers.control, cx);
                        } else if event.pressed_button == Some(MouseButton::Middle) {
                            this.dismiss_help(cx);
                            this.pan_hue(event.position, cx);
                        } else if event.pressed_button.is_none() {
                            this.arm_help(event.position, window, cx);
                        } else {
                            this.dismiss_help(cx);
                        }
                    }))
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        if !hovered {
                            this.dismiss_help(cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.dismiss_help(cx);
                            if matches!(
                                this.kind,
                                Kind::Plot(None) | Kind::Plot(Some(Plane::Ch | Plane::Lh))
                            ) {
                                this.hue_pan_start = Some(event.position);
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| this.dismiss_help(cx)),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, _, _, _| this.hue_pan_start = None),
                    )
                    .on_mouse_up_out(
                        MouseButton::Middle,
                        cx.listener(|this, _, _, _| this.hue_pan_start = None),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| {
                            this.spline_drag = None;
                            this.orbit_start = None;
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| {
                            this.spline_drag = None;
                            this.orbit_start = None;
                        }),
                    )
                    .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                        this.dismiss_help(cx);
                        if plane.is_none() {
                            let delta = event.delta.pixel_delta(px(20.));
                            this.model.update(cx, |m, cx| {
                                m.project.zoom =
                                    (m.project.zoom + f32::from(delta.y) * 0.002).clamp(0.5, 2.5);
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }
                    })),
            )
            .child(bottom)
            .into_any_element()
    }
    fn local(&self, point: Point<Pixels>) -> [f64; 2] {
        [
            (f32::from(point.x - self.bounds.left()) / f32::from(self.bounds.size.width).max(1.))
                as f64,
            1. - (f32::from(point.y - self.bounds.top())
                / f32::from(self.bounds.size.height).max(1.)) as f64,
        ]
    }
    fn mouse_down(&mut self, pos: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_help(cx);
        let Kind::Plot(plane) = self.kind else {
            return;
        };
        if plane.is_none() {
            self.orbit_start = Some(pos);
            return;
        }
        let plane = plane.unwrap();
        let p = self.local(pos);
        let m = self.model.read(cx);
        if m.project.mode == Mode::Spline(plane)
            && let Some(spline) = &m.project.spline
        {
            let w = f32::from(self.bounds.size.width) as f64;
            let h = f32::from(self.bounds.size.height) as f64;
            let hit = plane
                .spline_images(spline, m.project.hue_origin)
                .iter()
                .find_map(|image| spline::hit_test(image, p, [w, h]));
            if let Some(target) = hit {
                self.selected_node = Some(target.node());
                self.spline_drag =
                    Some(Drag::new(spline.clone(), target, p).cyclic_x(plane.axes().0 == 2));
                window.focus(&self.focus, cx);
                cx.notify();
                return;
            }
        }
        let nearest = m
            .curve
            .sample(CURVE_SAMPLES)
            .into_iter()
            .min_by(|a, b| {
                let dist = |s: &Sample| {
                    let q = plane.project_at(s.color, m.project.hue_origin);
                    (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)
                };
                dist(a).total_cmp(&dist(b))
            })
            .unwrap();
        self.model.update(cx, |m, cx| {
            m.project.cursor = nearest.t;
            cx.notify();
        });
    }
    fn mouse_move(&mut self, pos: Point<Pixels>, ctrl: bool, cx: &mut Context<Self>) {
        if let Some(last) = self.orbit_start {
            let d = pos - last;
            self.orbit_start = Some(pos);
            self.model.update(cx, |m, cx| {
                m.project.orbit(f32::from(d.x), f32::from(d.y));
                cx.notify();
            });
        }
        let p = self.local(pos);
        let metric = [
            f32::from(self.bounds.size.width) as f64,
            f32::from(self.bounds.size.height) as f64,
        ];
        if let (Some(drag), Kind::Plot(Some(plane))) = (&mut self.spline_drag, self.kind) {
            if self.model.read(cx).project.mode != Mode::Spline(plane) {
                self.spline_drag = None;
                return;
            }
            let spline = drag.update(p, ctrl, metric);
            self.model.update(cx, |m, cx| {
                m.project.spline = Some(spline);
                m.rebuild(cx);
            });
        }
    }
    fn pan_hue(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(previous) = self.hue_pan_start {
            let dx = f32::from(pos.x - previous.x) / f32::from(self.bounds.size.width).max(1.);
            self.hue_pan_start = Some(pos);
            self.model.update(cx, |m, cx| {
                m.project.pan_hue(dx as f64);
                cx.notify();
            });
        }
    }
    fn dismiss_help(&mut self, cx: &mut Context<Self>) {
        self.help_timer = None;
        self.help_position = None;
        if self.help_tooltip.take().is_some() {
            cx.notify();
        }
    }
    fn arm_help(&mut self, pos: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.help_position == Some(pos) {
            return;
        }
        self.dismiss_help(cx);
        self.help_position = Some(pos);
        self.help_timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(PLOT_HELP_DELAY).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.help_position != Some(pos)
                    || window.mouse_position() != pos
                    || !this.bounds.contains(&pos)
                    || window.last_input_was_keyboard()
                {
                    return;
                }
                let text = this.plot_help(cx);
                let weak = cx.weak_entity();
                this.help_tooltip = Some(AnyTooltip {
                    view: Tooltip::new(text).max_w(px(380.)).build(window, cx),
                    mouse_position: pos,
                    check_visible_and_update: Rc::new(move |_, window, cx| {
                        weak.upgrade().is_some_and(|panel| {
                            let panel = panel.read(cx);
                            panel.help_position == Some(pos)
                                && panel.help_tooltip.is_some()
                                && panel.bounds.contains(&window.mouse_position())
                                && window.mouse_position() == pos
                                && !window.last_input_was_keyboard()
                        })
                    }),
                });
                cx.notify();
            });
        }));
    }
    fn plot_help(&self, cx: &App) -> String {
        let Kind::Plot(plane) = self.kind else {
            return String::new();
        };
        let p = &self.model.read(cx).project;
        let Some(plane) = plane else {
            return format!(
                "Left-drag to orbit. Zenith ranges from 0° straight above through 90° horizontal to 180° straight below. Use the projection toggle for parallel orthographic rays. Middle-drag horizontally to move the hue origin. Scroll to zoom.\n{}\nSlice contours show L in blue, C in gold, H in purple. Adjust them with the projection sliders.\nCoral dashes mark the path outside sRGB.",
                if p.polar {
                    "Chroma is radius, hue is azimuth. The vertical C=0 axis runs from black to white."
                } else {
                    "Hue and lightness form the floor; chroma is height. Surface colors follow the sRGB boundary."
                }
            );
        };
        let mut help="Click the path to inspect its parameter. Colored dots mark intersections with this slice. Coral dashes mark colors outside sRGB.".to_string();
        if plane.axes().0 == 2 {
            help.push_str("\nMiddle-drag horizontally to pan the shared cyclic hue origin.");
        }
        if p.mode == Mode::Spline(plane) {
            help.push_str("\nDrag circular anchors or square handles. Ctrl locks anchor movement to x/y and snaps handles to x/y/their original direction. Moving an anchor preserves its handle offsets. Smooth joins adjust the middle parameter to retain C1.");
            if p.spline_constant {
                help.push_str(&format!("\nConstant mode: the {} slice slider also sets this channel for the whole colormap. Toggle constant mode off to restore its expression.",model::axis_name(plane.axes().2)));
            }
        } else {
            help.push_str("\nUse the pencil button to enter or leave spline editing. A lock means another plane or the reference path owns the curve.");
        }
        help
    }
}
impl BasePanel for StudioPanel {
    fn panel_name(&self) -> &'static str {
        match self.kind {
            Kind::Editor => "studio.editor",
            Kind::Output => "studio.output",
            Kind::Import => "studio.import",
            Kind::Plot(Some(Plane::Cl)) => "studio.cl",
            Kind::Plot(Some(Plane::Ch)) => "studio.ch",
            Kind::Plot(Some(Plane::Lh)) => "studio.lh",
            Kind::Plot(None) => "studio.3d",
        }
    }
    fn closable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for StudioPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .child(self.kind.title())
    }
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}
impl EventEmitter<PanelEvent> for StudioPanel {}
impl Focusable for StudioPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for StudioPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.kind {
            Kind::Editor => self.editor(cx),
            Kind::Output => self.output(window, cx),
            Kind::Import => self.importer(cx),
            Kind::Plot(plane) => self.plot(plane, cx),
        }
    }
}

fn line(a: Point<Pixels>, b: Point<Pixels>, width: f32, color: u32, window: &mut Window) {
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(a);
    path.line_to(b);
    if let Ok(path) = path.build() {
        window.paint_path(path, rgb(color));
    }
}
fn circle(p: Point<Pixels>, r: f32, color: Hsla, border: Option<(f32, Hsla)>, window: &mut Window) {
    let mut q = fill(
        Bounds::new(
            point(p.x - px(r), p.y - px(r)),
            size(px(r * 2.), px(r * 2.)),
        ),
        color,
    );
    q.corner_radii = Corners::all(px(r));
    if let Some((w, c)) = border {
        q.border_widths = Edges::all(px(w));
        q.border_color = c;
    }
    window.paint_quad(q);
}
fn label(text: String, p: Point<Pixels>, color: u32, window: &mut Window, cx: &mut App) {
    let len = text.len();
    let line = window.text_system().shape_line(
        text.into(),
        px(11.),
        &[TextRun {
            len,
            font: font("Consolas"),
            color: rgb(color).into(),
            background_color: Some(rgb(palette(cx).label_background).into()),
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let _ = line.paint(p, px(15.), TextAlign::Left, None, window, cx);
}
fn outlined_label(text: String, p: Point<Pixels>, window: &mut Window, cx: &mut App) {
    let (foreground, outline) = match appearance(cx) {
        Appearance::Dark => (0xffffff, 0x000000),
        Appearance::Light => (0x000000, 0xffffff),
    };
    let shape = |color| {
        window.text_system().shape_line(
            text.clone().into(),
            px(11.),
            &[TextRun {
                len: text.len(),
                font: font("Consolas"),
                color: rgb(color).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
    };
    let halo = shape(outline);
    let fill = shape(foreground);
    // A one-pixel glyph halo stays visible over every color in the bar.
    for (dx, dy) in [
        (-1., 0.),
        (1., 0.),
        (0., -1.),
        (0., 1.),
        (-0.707, -0.707),
        (-0.707, 0.707),
        (0.707, -0.707),
        (0.707, 0.707),
    ] {
        let _ = halo.paint(
            point(p.x + px(dx), p.y + px(dy)),
            px(15.),
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
    let _ = fill.paint(p, px(15.), TextAlign::Left, None, window, cx);
}
fn paint_plot(
    bounds: Bounds<Pixels>,
    plane: Option<Plane>,
    p: &Project,
    curve: &Curve,
    selected: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let colors = palette(cx);
    let aspect = f32::from(bounds.size.width) / f32::from(bounds.size.height).max(1.);
    let project = |c: Oklch| -> Option<Point<Pixels>> {
        let q = if let Some(plane) = plane {
            plane.project_at(c, p.hue_origin)
        } else {
            gpu::project3(c, p, aspect)?
        };
        if !q.iter().all(|x| x.is_finite() && x.abs() < 1e5) {
            return None;
        }
        Some(point(
            bounds.left() + bounds.size.width * q[0] as f32,
            bounds.bottom() - bounds.size.height * q[1] as f32,
        ))
    };
    if let Some(plane) = plane {
        let (x, y, _) = plane.axes();
        let zero_x =
            bounds.left() + bounds.size.width * model::hue_position(0., p.hue_origin) as f32;
        for i in 0..=4 {
            let v = i as f32 / 4.;
            let hue_tick = (p.hue_origin + i as f64 * 90.).rem_euclid(360.);
            let zero_tick = x == 2 && hue_tick.abs() < 1e-10;
            let tick_x = (bounds.left() + bounds.size.width * v).min(bounds.right() - px(32.));
            let x_label = if x == 2 {
                format!("{hue_tick:.0}°")
            } else {
                format!("{v:.2}")
            };
            let y_label = if y == 1 {
                format!("{:.2}", v * 0.4)
            } else {
                format!("{v:.2}")
            };
            // The dedicated zero-hue marker supplies a coincident tick label.
            if !zero_tick || f32::from(tick_x - zero_x).abs() > 24. {
                label(
                    x_label,
                    point(tick_x, bounds.top() + px(if zero_tick { 18. } else { 2. })),
                    colors.muted,
                    window,
                    cx,
                );
            }
            if i > 0 && i < 4 {
                label(
                    y_label,
                    point(
                        bounds.left() + px(4.),
                        (bounds.bottom() - bounds.size.height * v).max(bounds.top() + px(2.)),
                    ),
                    colors.muted,
                    window,
                    cx,
                );
            }
        }
        label(
            model::axis_name(y).into(),
            point(
                bounds.left() + px(5.),
                bounds.top() + px(if x == 2 { 34. } else { 18. }),
            ),
            colors.text,
            window,
            cx,
        );
        label(
            model::axis_name(x).into(),
            point(
                bounds.right() - px(14.),
                bounds.top() + px(if x == 2 { 34. } else { 18. }),
            ),
            colors.text,
            window,
            cx,
        );
        if x == 2 {
            line(
                point(zero_x, bounds.top()),
                point(zero_x, bounds.bottom()),
                1.,
                colors.guide,
                window,
            );
            label(
                "0°".into(),
                point(
                    (zero_x + px(4.)).min(bounds.right() - px(24.)),
                    bounds.top() + px(18.),
                ),
                colors.channels[2],
                window,
                cx,
            );
        }
    } else if p.polar {
        let axis_color = colors.channels[0];
        if let (Some(a), Some(b)) = (
            project(Oklch::new(0., 0., 0.)),
            project(Oklch::new(1., 0., 0.)),
        ) {
            line(a, b, 1.3, axis_color, window);
            label(
                "L=1 · white".into(),
                point(b.x + px(7.), b.y - px(15.)),
                colors.text,
                window,
                cx,
            );
            if f32::from(a.x - b.x).hypot(f32::from(a.y - b.y)) > 24. {
                label(
                    "L=0".into(),
                    point(a.x + px(7.), a.y),
                    colors.muted,
                    window,
                    cx,
                );
            }
        }
        for i in 0..96 {
            let h = i as f64 * 360. / 96.;
            if let (Some(a), Some(b)) = (
                project(Oklch::new(0., 0.4, h)),
                project(Oklch::new(0., 0.4, h + 360. / 96.)),
            ) {
                line(a, b, 1., colors.border, window);
            }
        }
        for h in [0., 90., 180., 270.] {
            if let (Some(a), Some(b)) = (
                project(Oklch::new(0., 0., h)),
                project(Oklch::new(0., 0.4, h)),
            ) {
                line(a, b, 1., colors.border, window);
                label(format!("{h:.0}°"), b, colors.muted, window, cx);
                if h == 0. {
                    circle(b, 4., rgb(colors.channels[2]).into(), None, window);
                }
            }
        }
    } else {
        // Bounding box and coordinate axes make the rectangular L–H floor explicit.
        for &(a, b, color) in &[
            (
                Oklch::new(0., 0., p.hue_origin),
                Oklch::new(1., 0., p.hue_origin),
                colors.channels[0],
            ),
            (
                Oklch::new(0., 0., p.hue_origin),
                Oklch::new(0., 0.4, p.hue_origin),
                colors.channels[1],
            ),
        ] {
            if let (Some(a), Some(b)) = (project(a), project(b)) {
                line(a, b, 1., color, window);
            }
        }
        let project_world = |v: glam::Vec3| {
            let q = gpu::camera(p, aspect) * v.extend(1.);
            point(
                bounds.left() + bounds.size.width * (q.x / q.w * 0.5 + 0.5),
                bounds.bottom() - bounds.size.height * (q.y / q.w * 0.5 + 0.5),
            )
        };
        for l in [0., 1.] {
            line(
                project_world(glam::Vec3::new(-0.9, -0.3, l * 1.3 - 0.65)),
                project_world(glam::Vec3::new(0.9, -0.3, l * 1.3 - 0.65)),
                1.,
                colors.border,
                window,
            );
        }
        for (v, s, col) in [
            (glam::Vec3::new(0.92, -0.3, -0.65), "H", colors.channels[2]),
            (glam::Vec3::new(-0.9, -0.3, 0.70), "L", colors.channels[0]),
            (glam::Vec3::new(-0.9, 0.92, -0.65), "C", colors.channels[1]),
        ] {
            label(s.into(), project_world(v), col, window, cx);
        }
        if let Some(zero) = project(Oklch::new(0., 0., 0.)) {
            circle(zero, 4., rgb(colors.channels[2]).into(), None, window);
            label(
                "0°".into(),
                point(zero.x + px(6.), zero.y),
                colors.channels[2],
                window,
                cx,
            );
        }
    }
    for col in &p.imported {
        if let Some(pos) = project(*col) {
            let d = if let Some(plane) = plane {
                let a = plane.axes().2;
                let mut d = (col.channel(a) - p.slices[a]).abs();
                if a == 2 {
                    d = d.rem_euclid(360.).min(360. - d.rem_euclid(360.));
                }
                d / match a {
                    1 => 0.4,
                    2 => 180.,
                    _ => 1.,
                }
            } else {
                0.
            };
            let r = (3. + d.min(1.) * 13.) as f32;
            circle(
                pos,
                r,
                Hsla::from(color_value(*col)).alpha((0.85 / (1. + d * 12.)) as f32),
                Some((
                    1.,
                    Hsla::from(rgb(colors.curve)).alpha((0.7 / (1. + d * 8.)) as f32),
                )),
                window,
            );
        }
    }
    let samples = curve.sample(CURVE_SAMPLES);
    let mut strokes: Vec<(bool, Vec<Point<Pixels>>)> = vec![];
    let mut points: Vec<Point<Pixels>> = vec![];
    let mut last_outside = false;
    let mut distance = 0f32;
    let flush = |points: &mut Vec<Point<Pixels>>,
                 outside: bool,
                 strokes: &mut Vec<(bool, Vec<Point<Pixels>>)>| {
        if points.len() > 1 {
            strokes.push((outside, std::mem::take(points)));
        } else {
            points.clear();
        }
    };
    for pair in samples.windows(2) {
        let a = pair[0].color;
        let b = pair[1].color;
        if !p.connected_in_view(a, b, plane) {
            flush(&mut points, last_outside, &mut strokes);
            continue;
        }
        let outside = !a.in_gamut() || !b.in_gamut();
        if let (Some(a), Some(b)) = (project(a), project(b)) {
            distance += f32::from(b.x - a.x).hypot(f32::from(b.y - a.y));
            if outside && (distance / 7.).floor() as u32 % 2 == 1 {
                flush(&mut points, last_outside, &mut strokes);
                continue;
            }
            if outside != last_outside {
                flush(&mut points, last_outside, &mut strokes);
            }
            last_outside = outside;
            if points.is_empty() {
                points.push(a);
            }
            points.push(b);
        }
    }
    flush(&mut points, last_outside, &mut strokes);
    for (outside, points) in strokes {
        for (width, color) in [
            (4.6, colors.curve_outline),
            (
                2.,
                if outside {
                    colors.warning
                } else {
                    colors.curve
                },
            ),
        ] {
            let mut path = PathBuilder::stroke(px(width));
            path.move_to(points[0]);
            for point in &points[1..] {
                path.line_to(*point);
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(color));
            }
        }
    }
    if let Some(plane) = plane
        && p.mode == Mode::Spline(plane)
        && let Some(spline) = &p.spline
    {
        let screen = |q: [f64; 2]| {
            point(
                bounds.left() + bounds.size.width * q[0] as f32,
                bounds.bottom() - bounds.size.height * q[1] as f32,
            )
        };
        if spline.symmetric {
            let x = if plane.axes().0 == 2 {
                model::hue_position(spline.axis() * 360., p.hue_origin)
            } else {
                spline.axis()
            };
            let height = f32::from(bounds.size.height);
            for i in 0..(height / 10.).ceil() as usize {
                let t = i as f64 * 10. / height as f64;
                line(
                    screen([x, t]),
                    screen([x, (t + 5. / height as f64).min(1.)]),
                    1.,
                    colors.accent,
                    window,
                );
            }
        }
        for (copy, image) in plane
            .spline_images(spline, p.hue_origin)
            .into_iter()
            .enumerate()
        {
            let spline = &image;
            if !spline.linear {
                for (i, node) in spline.nodes.iter().enumerate() {
                    for side in [Side::Incoming, Side::Outgoing] {
                        if (i == 0 && side == Side::Incoming)
                            || (i == spline.nodes.len() - 1 && side == Side::Outgoing)
                        {
                            continue;
                        }
                        if let Some(handle) = node.handle_position(side) {
                            let pos = screen(handle);
                            line(screen(node.position), pos, 1.2, colors.accent, window);
                            let mut square = fill(
                                Bounds::new(
                                    point(pos.x - px(4.), pos.y - px(4.)),
                                    size(px(8.), px(8.)),
                                ),
                                rgb(colors.accent),
                            );
                            square.border_widths = Edges::all(px(1.));
                            square.border_color = rgb(colors.surface).into();
                            window.paint_quad(square);
                        }
                    }
                }
            }
            for (i, node) in spline.nodes.iter().enumerate() {
                if copy != 0 {
                    continue;
                }
                let pos = screen(node.position);
                circle(
                    pos,
                    7.,
                    rgb(if selected == Some(i) {
                        colors.accent
                    } else {
                        colors.surface
                    })
                    .into(),
                    Some((2., rgb(colors.accent).into())),
                    window,
                );
                // Keep endpoint labels inside the viewport.
                let dx = if node.position[0] > 0.94 { -20. } else { 10. };
                let dy = if node.position[1] > 0.94 { 8. } else { -20. };
                label(
                    (i + 1).to_string(),
                    point(pos.x + px(dx), pos.y + px(dy)),
                    colors.accent,
                    window,
                    cx,
                );
            }
        }
    }
    for (i, slice) in Plane::ALL.into_iter().enumerate() {
        if plane.is_some_and(|v| v != slice) {
            continue;
        }
        let intersections = curve.intersections(slice, p.slices[slice.axes().2]);
        let intersections_colors = [colors.channels[2], colors.channels[0], colors.channels[1]];
        for s in intersections.points.iter().take(32) {
            if let Some(pos) = project(s.color) {
                circle(
                    pos,
                    5.,
                    rgb(intersections_colors[i]).into(),
                    Some((1.5, rgb(colors.surface).into())),
                    window,
                );
                label(
                    format!("{}={:.3}", p.parameter, s.t),
                    point(pos.x + px(8.), pos.y + px(5.)),
                    intersections_colors[i],
                    window,
                    cx,
                );
            }
        }
    }
    if let Some(pos) = project(curve.at(p.cursor)) {
        circle(
            pos,
            5.,
            rgb(colors.curve).into(),
            Some((2., rgb(colors.curve_outline).into())),
            window,
        );
    }
}

pub struct Studio {
    model: Entity<StudioModel>,
    dock: Entity<DockArea>,
    panels: Vec<Entity<StudioPanel>>,
    _observer: Subscription,
}
impl Studio {
    pub fn new(gpu: Arc<Gpu>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = cx.new(|cx| StudioModel::new(gpu, window, cx));
        let panels = [
            Kind::Editor,
            Kind::Plot(Some(Plane::Cl)),
            Kind::Plot(Some(Plane::Ch)),
            Kind::Plot(None),
            Kind::Plot(Some(Plane::Lh)),
            Kind::Output,
            Kind::Import,
        ]
        .into_iter()
        .map(|kind| cx.new(|cx| StudioPanel::new(kind, model.clone(), cx)))
        .collect();
        let dock = cx.new(|cx| {
            DockArea::new("okclrmp", Some(1), window, cx).with_renderer(DockSkin::new(cx))
        });
        let observer = cx.observe(&model, |_, _, cx| cx.notify());
        let mut this = Self {
            model,
            dock,
            panels,
            _observer: observer,
        };
        this.reset_layout(window, cx);
        this
    }
    fn reset_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let p = &self.panels;
        let tabs = |i: usize| DockLayout::tabs().panel_view(dock::panel_handle(p[i].clone()), cx);
        let center = DockLayout::v_split()
            .child(
                DockLayout::h_split()
                    .child(tabs(1), None)
                    .child(tabs(2), None)
                    .child(tabs(4), None),
                None,
            )
            .child(
                DockLayout::h_split()
                    .child(tabs(5), None)
                    .child(tabs(3), None),
                None,
            );
        let left = tabs(0);
        let right = tabs(6);
        self.dock.update(cx, |area, cx| {
            for placement in [
                DockPlacement::Left,
                DockPlacement::Right,
                DockPlacement::Bottom,
            ] {
                area.remove_dock(placement, window, cx);
            }
            area.set_center(center, window, cx);
            area.set_dock(DockPlacement::Left, left, window, cx);
            area.set_dock_size(DockPlacement::Left, px(310.), window, cx);
            area.set_dock(DockPlacement::Right, right, window, cx);
            area.set_dock_size(DockPlacement::Right, px(292.), window, cx);
            if area.is_dock_open(DockPlacement::Right) {
                area.toggle_dock(DockPlacement::Right, window, cx);
            }
        });
    }
    fn save_project(&mut self, cx: &mut Context<Self>) {
        let m = self.model.read(cx);
        if m.error.is_some() {
            return;
        }
        let text = serde_json::to_string_pretty(&m.project).unwrap();
        let model = self.model.clone();
        let prompt = cx.prompt_for_new_path(
            &std::env::current_dir().unwrap_or_default(),
            Some("colormap.okclrmp.json"),
        );
        cx.spawn(async move |_, cx| {
            if let Ok(Ok(Some(path))) = prompt.await {
                let message = match std::fs::write(&path, text) {
                    Ok(_) => format!("Project saved to {}", path.display()),
                    Err(e) => format!("Save failed: {e}"),
                };
                model.update(cx, |m, cx| {
                    m.notice = message;
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open OKLCH project".into()),
        });
        let model = self.model.clone();
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await
                && let Some(path) = paths.first()
            {
                let result = std::fs::read_to_string(path)
                    .map_err(|e| e.to_string())
                    .and_then(|s| serde_json::from_str::<Project>(&s).map_err(|e| e.to_string()));
                let _ = this.update_in(cx, |_, window, cx| {
                    model.update(cx, |m, cx| {
                        match result {
                            Ok(p) => {
                                if let Err(e) = m.set_project(p, window, cx) {
                                    m.notice = format!("Invalid project: {e}");
                                } else {
                                    m.notice = format!("Opened {}", path.display());
                                }
                            }
                            Err(e) => m.notice = format!("Open failed: {e}"),
                        };
                        cx.notify();
                    })
                });
            }
        })
        .detach();
    }
}
impl Render for Studio {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = palette(cx);
        let m = self.model.read(cx);
        let outside = m
            .curve
            .sample(256)
            .iter()
            .filter(|s| !s.color.in_gamut())
            .count();
        column()
            .size_full()
            .gap_0()
            .bg(rgb(colors.background))
            .text_color(rgb(colors.text))
            .text_sm()
            .child(
                row()
                    .h(px(52.))
                    .px_5()
                    .gap_4()
                    .border_b_1()
                    .border_color(rgb(colors.border))
                    .child(
                        row().gap_1().children(
                            [0xabc4b2, 0x60a391, 0x386557, 0x244038]
                                .into_iter()
                                .map(|c| div().w(px(6.)).h(px(24.)).rounded_sm().bg(rgb(c))),
                        ),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("OKLCH")
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(colors.muted))
                                    .child("COLORMAP STUDIO"),
                            ),
                    )
                    .child(div().w(px(1.)).h_8().bg(rgb(colors.border)))
                    .child(
                        caption(m.project.name.clone(), cx)
                            .flex_1()
                            .min_w_0()
                            .truncate(),
                    )
                    .child(
                        Button::new("toggle-theme")
                            .label(if cx.theme().is_dark() {
                                "Light theme"
                            } else {
                                "Dark theme"
                            })
                            .tooltip(
                                "Switch the app UI and plot backgrounds between light and dark",
                            )
                            .small()
                            .on_click(cx.listener(|this, _, window, cx| {
                                let mode = if cx.theme().is_dark() {
                                    ThemeMode::Light
                                } else {
                                    ThemeMode::Dark
                                };
                                Theme::change(mode, Some(window), cx);
                                this.model.update(cx, |_, cx| cx.notify());
                            })),
                    )
                    .child(
                        Button::new("open-project").label("Open…").small().on_click(
                            cx.listener(|this, _, window, cx| this.open_project(window, cx)),
                        ),
                    )
                    .child(
                        Button::new("save-project")
                            .label("Save project…")
                            .small()
                            .disabled(m.error.is_some())
                            .on_click(cx.listener(|this, _, _, cx| this.save_project(cx))),
                    )
                    .child(
                        Button::new("show-import")
                            .label("Reference colors")
                            .small()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dock.update(cx, |area, cx| {
                                    area.toggle_dock(DockPlacement::Right, window, cx)
                                })
                            })),
                    )
                    .child(
                        Button::new("reset-layout")
                            .label("Reset layout")
                            .small()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.reset_layout(window, cx)),
                            ),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.dock.clone()))
            .child(
                row()
                    .h(px(28.))
                    .px_4()
                    .border_t_1()
                    .border_color(rgb(colors.border))
                    .text_xs()
                    .child(
                        div()
                            .w(px(6.))
                            .h(px(6.))
                            .rounded_full()
                            .bg(rgb(if m.error.is_some() {
                                colors.warning
                            } else {
                                colors.accent
                            })),
                    )
                    .child(
                        caption(
                            if m.error.is_some() {
                                "Invalid expression · last valid path".into()
                            } else {
                                m.notice.clone()
                            },
                            cx,
                        )
                        .flex_1()
                        .truncate(),
                    )
                    .child(caption(
                        format!(
                            "{CURVE_SAMPLES} path samples · {:.1}% outside sRGB",
                            outside as f64 / 256. * 100.
                        ),
                        cx,
                    ))
                    .child(caption(format!("WGPU · {}", m.gpu.adapter_name), cx)),
            )
    }
}
