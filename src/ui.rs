use gpui_kit::component::{
    Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    dock::{self, BasePanel, DockArea, DockLayout, DockPlacement, DockSkin, Panel, PanelEvent},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    slider::{Slider, SliderEvent, SliderState},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use okclrmp::{
    color::Oklch,
    exchange::{self, ExportSpace, ImportSpace},
    gpu::{self, Gpu},
    model::{self, CURVE_SAMPLES, Curve, Mode, Plane, Project, Sample},
    spline::{self, Drag, NodeMode, Side, Spline},
};
use std::sync::Arc;

const BG: u32 = 0x0c0f14;
const SURFACE: u32 = 0x11161e;
const BORDER: u32 = 0x28313e;
const TEXT: u32 = 0xdce4ed;
const MUTED: u32 = 0x8492a6;
const ACCENT: u32 = 0xa9d8bd;
const WARNING: u32 = 0xff987e;
fn column() -> Div {
    div().flex().flex_col().gap_3()
}
fn row() -> Div {
    div().flex().items_center().gap_2()
}
fn caption(s: impl Into<SharedString>) -> Div {
    div().text_xs().text_color(rgb(MUTED)).child(s.into())
}
fn section(s: &str) -> Div {
    div()
        .mt_2()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(ACCENT))
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
    constants: Entity<InputState>,
    channels: [Entity<InputState>; 3],
    sliders: [Entity<SliderState>; 4],
    import_text: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}
impl StudioModel {
    pub fn new(gpu: Arc<Gpu>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = Project::default();
        let curve = project.compile().unwrap();
        let parameter =
            cx.new(|cx| InputState::new(window, cx).default_value(project.parameter.clone()));
        let constants =
            cx.new(|cx| InputState::new(window, cx).default_value(project.constants.clone()));
        let channels = std::array::from_fn(|i| {
            cx.new(|cx| InputState::new(window, cx).default_value(project.expressions[i].clone()))
        });
        let sliders = std::array::from_fn(|i| {
            cx.new(|_| {
                SliderState::new()
                    .min(0.)
                    .max([1., 0.4, 360., 1.][i])
                    .step([0.001, 0.001, 0.1, 0.001][i])
                    .default_value(if i < 3 {
                        project.slices[i] as f32
                    } else {
                        project.cursor as f32
                    })
            })
        });
        let import_text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(8)
                .default_value("#440154\n#414487\n#2a788e\n#22a884\n#7ad151\n#fde725")
        });
        let mut subscriptions = vec![];
        for input in [
            &parameter,
            &constants,
            &channels[0],
            &channels[1],
            &channels[2],
        ] {
            subscriptions.push(cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.read_inputs(cx);
                }
            }));
        }
        for (i, slider) in sliders.iter().enumerate() {
            subscriptions.push(
                cx.subscribe(slider, move |this, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Change(value) = event {
                        if i < 3 {
                            this.project.slices[i] = value.start() as f64;
                        } else {
                            this.project.cursor = value.start() as f64;
                        }
                        cx.notify();
                    }
                }),
            );
        }
        Self {
            project,
            curve,
            error: None,
            notice: "Drag a spline node to shape the map. Drag panel tabs to rearrange.".into(),
            gpu,
            parameter,
            constants,
            channels,
            sliders,
            import_text,
            _subscriptions: subscriptions,
        }
    }
    fn read_inputs(&mut self, cx: &mut Context<Self>) {
        self.project.parameter = self.parameter.read(cx).value().to_string();
        self.project.constants = self.constants.read(cx).value().to_string();
        self.project.expressions =
            std::array::from_fn(|i| self.channels[i].read(cx).value().to_string());
        self.rebuild(cx);
    }
    fn rebuild(&mut self, cx: &mut Context<Self>) {
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
        let curve = p.compile()?;
        self.project = p;
        self.curve = curve;
        self.error = None;
        self.parameter.update(cx, |s, cx| {
            s.set_value(self.project.parameter.clone(), window, cx)
        });
        self.constants.update(cx, |s, cx| {
            s.set_value(self.project.constants.clone(), window, cx)
        });
        for i in 0..3 {
            self.channels[i].update(cx, |s, cx| {
                s.set_value(self.project.expressions[i].clone(), window, cx)
            });
        }
        for i in 0..4 {
            let v = if i < 3 {
                self.project.slices[i]
            } else {
                self.project.cursor
            };
            self.sliders[i].update(cx, |s, cx| s.set_value(v as f32, window, cx));
        }
        cx.notify();
        Ok(())
    }
    fn sync_slices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let c = self.curve.at(self.project.cursor);
        self.project.slices = [c.l.clamp(0., 1.), c.c.clamp(0., 0.4), c.h.rem_euclid(360.)];
        for i in 0..3 {
            self.sliders[i].update(cx, |s, cx| {
                s.set_value(self.project.slices[i] as f32, window, cx)
            });
        }
        cx.notify();
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
    export_space: ExportSpace,
    import_space: ImportSpace,
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
            export_space: ExportSpace::Hex,
            import_space: ImportSpace::Hex,
        }
    }
    fn editor(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let m = self.model.read(cx);
        let p = m.project.clone();
        let current = m.curve.at(p.cursor);
        let mut content = column()
            .p_4()
            .gap_3()
            .child(caption("DESIGN A PATH THROUGH COLOR"))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(p.name.clone()),
            )
            .child(section("STARTING POINTS"));
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
            .child(section("CURVE DEFINITION"))
            .child(
                row()
                    .child(
                        Button::new("expr-mode")
                            .label("Expressions")
                            .small()
                            .selected(expressions)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |m, cx| {
                                    m.project.mode = Mode::Expressions;
                                    m.rebuild(cx);
                                })
                            })),
                    )
                    .child(caption(match p.mode {
                        Mode::Spline(plane) => format!("{} spline", plane.short()),
                        Mode::Imported => "Imported path".into(),
                        _ => "L · C · H".into(),
                    })),
            )
            .child(
                row().child(caption("Parameter").w_24()).child(
                    Input::new(&m.parameter)
                        .small()
                        .aria_label("Parameter name"),
                ),
            )
            .child(caption("One parameter on [0, 1]. Hue is in degrees."))
            .child(caption("Constants · name = value; …"))
            .child(
                Input::new(&m.constants)
                    .small()
                    .aria_label("Custom constants"),
            );
        for i in 0..3 {
            let locked = match p.mode {
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
                                    .text_color(rgb([0xaacfe5, 0xe4c27e, 0xc6b3e8][i]))
                                    .child(format!("{}({})", model::axis_name(i), p.parameter)),
                            )
                            .when(locked, |r| {
                                r.child(caption(if matches!(p.mode, Mode::Imported) {
                                    "from reference"
                                } else {
                                    "from spline"
                                }))
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
                        .bg(rgb(0x362522))
                        .text_xs()
                        .text_color(rgb(WARNING))
                        .child(error.clone()),
                )
                .child(caption(
                    "Plots show the last valid curve. Export is paused.",
                ));
        }
        if let Mode::Spline(plane) = p.mode
            && let Some(s) = &p.spline
        {
            content = content.child(section("SPLINE NODES"));
            for (i, n) in s.nodes.iter().enumerate() {
                let v = plane.from_point(n.position);
                content = content.child(
                    caption(format!(
                        "{} · {}={:.3}  ({:.4}, {:.4})",
                        i + 1,
                        p.parameter,
                        s.parameter(i),
                        v[0],
                        v[1]
                    ))
                    .font_family("Consolas"),
                );
            }
            content = content.child(caption("Edit nodes and handles in the active projection. Smooth keeps independent handle lengths; the middle parameter adjusts to preserve C1."));
        }
        content=content.child(section("INSPECT THE PATH"))
            .child(row().child(caption(format!("{} = {:.3}",p.parameter,p.cursor)).flex_1()).child(div().w_8().h_5().rounded_sm().bg(color_value(current))).child(caption(current.hex(true))))
            .child(Slider::new(&m.sliders[3]).accessibility_label("Curve parameter"))
            .child(caption(format!("L {:.4}   C {:.4}   H {:.2}°",current.l,current.c,current.h)).font_family("Consolas"))
            .child(Button::new("follow").label("Move slices to this color").small().w_full().on_click(cx.listener(|this,_,window,cx|this.model.update(cx,|m,cx|m.sync_slices(window,cx)))))
            .child(section("EXPRESSION NOTES"))
            .child(caption("+, −, *, /, ^ · comparisons · && · ||\ncondition ? a : b\nabs, sin, cos, sind, sqrt, min, max, clamp, mod, pi\nUse explicit * for multiplication."));
        div()
            .id("editor-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
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
    fn spline_tools(&self, s: &Spline, cx: &Context<Self>) -> Div {
        let mut tools = column().gap_1().child(
            row()
                .flex_wrap()
                .gap_1()
                .child(
                    Button::new("cubic")
                        .label("Cubic")
                        .small()
                        .selected(!s.linear)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_spline(cx, |s| s.set_linear(false))
                        })),
                )
                .child(
                    Button::new("linear")
                        .label("Line")
                        .small()
                        .selected(s.linear)
                        .tooltip("Straight line · two nodes · no handles")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_spline(cx, |s| s.set_linear(true))
                        })),
                )
                .child(
                    Button::new("two-nodes")
                        .label("2 nodes")
                        .small()
                        .selected(s.nodes.len() == 2)
                        .tooltip("Remove the middle node; preserve endpoint tangents")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_spline(cx, |s| s.remove_middle())
                        })),
                )
                .child(
                    Button::new("three-nodes")
                        .label("3 nodes")
                        .small()
                        .selected(s.nodes.len() == 3)
                        .tooltip("Split the cubic at its midpoint without changing its shape")
                        .on_click(
                            cx.listener(|this, _, _, cx| this.edit_spline(cx, |s| s.add_middle())),
                        ),
                )
                .child(
                    Button::new("symmetry")
                        .label("Symmetry")
                        .small()
                        .selected(s.symmetric)
                        .tooltip("Mirror nodes and handles about the vertical axis")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_spline(cx, |s| s.set_symmetry(!s.symmetric))
                        })),
                ),
        );
        if s.nodes.len() == 3 && !s.linear {
            let mut modes = row().flex_wrap().gap_1().child(caption("Middle"));
            for (i, mode, tip) in [
                (
                    0,
                    NodeMode::Corner,
                    "First click: free the tangents. Second click: remove middle handles.",
                ),
                (
                    1,
                    NodeMode::Smooth,
                    "Snap to C1; dragging preserves the opposite handle length.",
                ),
                (2, NodeMode::Auto, "Solve C2 now; manual edits keep C1."),
                (
                    3,
                    NodeMode::Symmetric,
                    "Keep C1 and equal lengths; dragging changes both handles.",
                ),
            ] {
                modes = modes.child(
                    Button::new(("middle-mode", i as usize))
                        .label(mode.label())
                        .small()
                        .selected(s.nodes[1].mode == mode)
                        .tooltip(tip)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_spline(cx, |s| s.set_middle_mode(mode))
                        })),
                );
            }
            tools = tools.child(modes);
        }
        tools
    }
    fn enable_spline(&mut self, plane: Plane, cx: &mut Context<Self>) {
        self.model.update(cx, |m, cx| {
            if matches!(m.project.mode, Mode::Expressions) {
                m.project.enable_spline(plane, &m.curve);
                m.rebuild(cx);
            }
        });
    }
    fn output(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let m = self.model.read(cx);
        let p = &m.project;
        let invalid = m.error.is_some();
        let count = p.sample_count;
        let data = exchange::export(&m.curve, count, self.export_space, p.map_chroma);
        let samples = m.curve.sample(count);
        let oog = samples.iter().filter(|s| !s.color.in_gamut()).count();
        let curve = m.curve.clone();
        let mapping = p.map_chroma;
        let param = p.parameter.clone();
        let cursor = p.cursor;
        let bar = canvas(
            |bounds, _, _| bounds,
            move |_, bounds, window, cx| {
                let steps = 512;
                for i in 0..steps {
                    let col = curve.at(i as f64 / (steps - 1) as f64);
                    let x = bounds.origin.x + bounds.size.width * (i as f32 / steps as f32);
                    let w = bounds.size.width / steps as f32 + px(0.5);
                    let a = col.rgba8(mapping);
                    window.paint_quad(fill(
                        Bounds::new(point(x, bounds.origin.y), size(w, bounds.size.height)),
                        rgb(((a[0] as u32) << 16) | ((a[1] as u32) << 8) | a[2] as u32),
                    ));
                    if !col.in_gamut() {
                        window.paint_quad(fill(
                            Bounds::new(point(x, bounds.bottom() - px(3.)), size(w, px(3.))),
                            rgb(WARNING),
                        ));
                    }
                }
                let x = bounds.origin.x + bounds.size.width * cursor as f32;
                line(
                    point(x, bounds.top()),
                    point(x, bounds.bottom()),
                    2.,
                    0xffffff,
                    window,
                );
                label(
                    format!("{param} = {cursor:.3}"),
                    point(
                        (x + px(5.)).min(bounds.right() - px(85.)),
                        bounds.top() + px(4.),
                    ),
                    0xffffff,
                    window,
                    cx,
                );
            },
        )
        .h(px(50.))
        .w_full();
        let mut controls = row().flex_wrap().gap_2().child(caption("Samples"));
        for n in [16, 64, 256, 1024] {
            controls = controls.child(
                Button::new(("count", n))
                    .label(n.to_string())
                    .small()
                    .selected(count == n)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.update(cx, |m, cx| {
                            m.project.sample_count = n;
                            cx.notify();
                        })
                    })),
            );
        }
        controls = controls.child(div().flex_1());
        for (i, space) in ExportSpace::ALL.into_iter().enumerate() {
            controls = controls.child(
                Button::new(("space", i))
                    .label(space.label())
                    .small()
                    .selected(self.export_space == space)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.export_space = space;
                        cx.notify();
                    })),
            );
        }
        let copy = data.clone();
        let preview = data.lines().take(3).collect::<Vec<_>>().join(" ");
        let content = column()
            .w_full()
            .p_3()
            .gap_2()
            .bg(rgb(SURFACE))
            .child(
                row()
                    .flex_wrap()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .child(format!("{count} colors · evenly sampled on [0, 1]")),
                    )
                    .child(caption(format!("{oog} outside sRGB")))
                    .child(
                        Button::new("mapping")
                            .label(if p.map_chroma {
                                "✓ Map chroma for RGB"
                            } else {
                                "Clip RGB channels"
                            })
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |m, cx| {
                                    m.project.map_chroma = !m.project.map_chroma;
                                    cx.notify();
                                })
                            })),
                    )
                    .child(
                        Button::new("copy-colors")
                            .label("Copy colors")
                            .small()
                            .primary()
                            .disabled(invalid)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let message = match crate::clipboard::copy_text(window, cx, &copy) {
                                    Ok(()) => "Colormap copied to clipboard".into(),
                                    Err(e) => format!("Copy failed: {e}"),
                                };
                                this.model.update(cx, |m, cx| {
                                    m.notice = message;
                                    cx.notify();
                                });
                            })),
                    )
                    .child(
                        Button::new("save-colors")
                            .label("Export file…")
                            .small()
                            .disabled(invalid)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.save_text(
                                    data.clone(),
                                    if matches!(
                                        this.export_space,
                                        ExportSpace::Hex | ExportSpace::Rgb
                                    ) {
                                        "colormap.json"
                                    } else {
                                        "colormap.csv"
                                    },
                                    cx,
                                );
                            })),
                    ),
            )
            .child(bar)
            .child(
                row()
                    .justify_between()
                    .child(caption("0.0"))
                    .child(caption("Coral underline = original path outside sRGB"))
                    .child(caption("1.0")),
            )
            .child(controls)
            .child(caption(preview).font_family("Consolas").truncate());
        div()
            .id("output-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
            .child(content)
            .into_any_element()
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
        let mut content=column().p_4().child(caption("COMPARE AN EXISTING COLORMAP"))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Reference colors"))
            .child(spaces)
            .child(caption("Paste hex colors, a JSON array of triples, or three CSV columns. OKLCH uses L 0–1, C, and H in degrees."))
            .child(Textarea::new(&m.import_text).h(px(180.)).font_family("Consolas").aria_label("Reference colors to import"))
            .child(row().child(Button::new("plot-reference").label("Plot colors").small().primary().on_click(cx.listener(|this,_,_,cx|this.import_colors(cx))))
                .child(Button::new("read-reference").label("Open file…").small().on_click(cx.listener(|this,_,window,cx|this.open_reference(window,cx)))))
            .child(caption(format!("{} reference points",imported.len())))
            .child(Button::new("use-reference").label("Use as colormap path").small().disabled(imported.len()<2).on_click(cx.listener(|this,_,_,cx|this.model.update(cx,|m,cx|{m.project.mode=Mode::Imported;m.rebuild(cx);m.notice="Reference path uses piecewise linear OKLCH interpolation with shortest hue arcs.".into();}))))
            .child(Button::new("clear-reference").label("Clear reference overlay").small().disabled(imported.is_empty()).on_click(cx.listener(|this,_,_,cx|this.model.update(cx,|m,cx|{
                if m.project.mode==Mode::Imported{m.project.mode=Mode::Expressions;}m.project.imported.clear();m.rebuild(cx);
            }))))
            .child(section("PROJECTION DEPTH"))
            .child(caption("Points near the slice are small and sharp. Distant points become larger, softer circles. All imported points remain in OKLCH."));
        for (i, c) in imported.iter().take(32).enumerate() {
            content = content.child(
                row()
                    .child(div().w_5().h_5().rounded_sm().bg(color_value(*c)))
                    .child(
                        caption(format!(
                            "{:02}  L {:.3}  C {:.3}  H {:.1}",
                            i + 1,
                            c.l,
                            c.c,
                            c.h
                        ))
                        .font_family("Consolas"),
                    ),
            );
        }
        div()
            .id("import-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
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
        let m = self.model.read(cx);
        let p = &m.project;
        let mut header = row().h(px(24.)).child(
            caption(if let Some(plane) = plane {
                let (_, _, a) = plane.axes();
                format!(
                    "{} = {:.3}{}",
                    model::axis_name(a),
                    p.slices[a],
                    if a == 2 { "°" } else { "" }
                )
            } else {
                if p.polar {
                    "C radius · H azimuth · L up"
                } else {
                    "H × L floor · C height"
                }
                .into()
            })
            .flex_1(),
        );
        if let Some(plane) = plane {
            let active = p.mode == Mode::Spline(plane);
            let locked =
                matches!(p.mode,Mode::Spline(other)if other!=plane) || p.mode == Mode::Imported;
            header = header.child(
                Button::new("spline")
                    .label(if active {
                        "● Editing spline"
                    } else if locked {
                        "Projected · locked"
                    } else {
                        "Edit spline"
                    })
                    .small()
                    .selected(active)
                    .disabled(locked)
                    .on_click(cx.listener(move |this, _, _, cx| this.enable_spline(plane, cx))),
            );
        } else {
            header = header
                .child(
                    Button::new("polar-view")
                        .label("Polar")
                        .small()
                        .selected(p.polar)
                        .tooltip("Lightness up · chroma radius · hue azimuth")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.project.polar = !m.project.polar;
                                cx.notify();
                            })
                        })),
                )
                .child(
                    Button::new("reset-camera")
                        .label("Reset view")
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.project.yaw = -0.65;
                                m.project.pitch = 0.65;
                                m.project.zoom = 1.;
                                m.project.hue_origin = 0.;
                                cx.notify();
                            })
                        })),
                );
        }
        let weak = cx.weak_entity();
        let canvas = canvas(
            move |bounds, window, cx| {
                weak.update(cx, |this, cx| {
                    this.bounds = bounds;
                    let m = this.model.read(cx);
                    let p = m.project.clone();
                    let [w, h] = m.gpu.image_size(
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                        window.scale_factor(),
                    );
                    let mut key = vec![w as u64, h as u64];
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
                        ]);
                    }
                    if this.cache.as_ref().is_none_or(|(k, _)| *k != key) {
                        match m.gpu.render(plane, &p, w, h) {
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
            let (_, _, a) = plane.axes();
            let intersections = m.curve.intersections(plane, p.slices[a]);
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
            bottom = bottom
                .when(plane.axes().0 == 2, |el| {
                    el.child(caption(
                        "Middle-drag = pan cyclic hue · linked to the other hue views",
                    ))
                })
                .when(p.mode == Mode::Spline(plane), |el| {
                    el.child(caption(
                        "Drag circles = nodes · squares = handles · Ctrl = axis / direction snap",
                    ))
                })
                .child(
                    Slider::new(&m.sliders[a])
                        .accessibility_label(format!("{} slice", model::axis_name(a))),
                )
                .child(caption(text).truncate());
        } else {
            bottom = bottom
                .child(caption(
                    "Left-drag = orbit · middle-drag = hue origin · scroll = zoom",
                ))
                .child(caption(if p.polar {
                    "C=0 is the vertical axis · white up · coral dashes = out of gamut"
                } else {
                    "Surface color = maximum chroma · coral dashes = out of gamut"
                }));
        }
        let mut content = column()
            .size_full()
            .min_h_0()
            .p_2()
            .gap_1()
            .bg(rgb(SURFACE))
            .child(header);
        if let Some(plane) = plane
            && p.mode == Mode::Spline(plane)
            && let Some(spline) = &p.spline
        {
            content = content.child(self.spline_tools(spline, cx));
        }
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
                    .border_color(rgb(BORDER))
                    .child(canvas)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.mouse_down(event.position, window, cx)
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.mouse_move(event.position, event.modifiers.control, cx);
                        } else if event.pressed_button == Some(MouseButton::Middle) {
                            this.pan_hue(event.position, cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            if matches!(
                                this.kind,
                                Kind::Plot(None) | Kind::Plot(Some(Plane::Ch | Plane::Lh))
                            ) {
                                this.hue_pan_start = Some(event.position);
                                cx.stop_propagation();
                            }
                        }),
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
            m.sliders[3].update(cx, |s, cx| s.set_value(nearest.t as f32, window, cx));
            cx.notify();
        });
    }
    fn mouse_move(&mut self, pos: Point<Pixels>, ctrl: bool, cx: &mut Context<Self>) {
        if let Some(last) = self.orbit_start {
            let d = pos - last;
            self.orbit_start = Some(pos);
            self.model.update(cx, |m, cx| {
                m.project.yaw -= f32::from(d.x) * 0.008;
                m.project.pitch = (m.project.pitch + f32::from(d.y) * 0.008).clamp(0.12, 1.45);
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.kind {
            Kind::Editor => self.editor(cx),
            Kind::Output => self.output(cx),
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
            background_color: Some(rgb(0x101722).into()),
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let _ = line.paint(p, px(15.), TextAlign::Left, None, window, cx);
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
        for i in 0..=4 {
            let v = i as f32 / 4.;
            let x_label = if x == 2 {
                format!("{:.0}°", (p.hue_origin + i as f64 * 90.).rem_euclid(360.))
            } else {
                format!("{v:.2}")
            };
            let y_label = if y == 1 {
                format!("{:.2}", v * 0.4)
            } else {
                format!("{v:.2}")
            };
            label(
                x_label,
                point(
                    (bounds.left() + bounds.size.width * v).min(bounds.right() - px(32.)),
                    bounds.bottom() - px(16.),
                ),
                MUTED,
                window,
                cx,
            );
            if i > 0 {
                label(
                    y_label,
                    point(
                        bounds.left() + px(4.),
                        (bounds.bottom() - bounds.size.height * v).max(bounds.top() + px(2.)),
                    ),
                    MUTED,
                    window,
                    cx,
                );
            }
        }
        label(
            model::axis_name(y).into(),
            point(bounds.left() + px(5.), bounds.top() + px(18.)),
            TEXT,
            window,
            cx,
        );
        label(
            model::axis_name(x).into(),
            point(bounds.right() - px(14.), bounds.bottom() - px(32.)),
            TEXT,
            window,
            cx,
        );
        if x == 2 {
            let zero =
                bounds.left() + bounds.size.width * model::hue_position(0., p.hue_origin) as f32;
            line(
                point(zero, bounds.top()),
                point(zero, bounds.bottom()),
                1.,
                0x344151,
                window,
            );
            label(
                "0°".into(),
                point(zero + px(4.), bounds.top() + px(2.)),
                0xc6b3e8,
                window,
                cx,
            );
        }
    } else if p.polar {
        let axis_color = 0xaacfe5;
        if let (Some(a), Some(b)) = (
            project(Oklch::new(0., 0., 0.)),
            project(Oklch::new(1., 0., 0.)),
        ) {
            line(a, b, 1.3, axis_color, window);
            label(
                "L=1 · white".into(),
                point(b.x + px(7.), b.y - px(15.)),
                TEXT,
                window,
                cx,
            );
            label("L=0".into(), point(a.x + px(7.), a.y), MUTED, window, cx);
        }
        for i in 0..96 {
            let h = i as f64 * 360. / 96.;
            if let (Some(a), Some(b)) = (
                project(Oklch::new(0., 0.4, h)),
                project(Oklch::new(0., 0.4, h + 360. / 96.)),
            ) {
                line(a, b, 1., BORDER, window);
            }
        }
        for h in [0., 90., 180., 270.] {
            if let (Some(a), Some(b)) = (
                project(Oklch::new(0., 0., h)),
                project(Oklch::new(0., 0.4, h)),
            ) {
                line(a, b, 1., BORDER, window);
                label(format!("{h:.0}°"), b, MUTED, window, cx);
                if h == 0. {
                    circle(b, 4., rgb(0xc6b3e8).into(), None, window);
                }
            }
        }
    } else {
        // Bounding box and coordinate axes make the rectangular L–H floor explicit.
        for &(a, b, color) in &[
            (
                Oklch::new(0., 0., p.hue_origin),
                Oklch::new(1., 0., p.hue_origin),
                0xaacfe5,
            ),
            (
                Oklch::new(0., 0., p.hue_origin),
                Oklch::new(0., 0.4, p.hue_origin),
                0xe4c27e,
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
                BORDER,
                window,
            );
        }
        for (v, s, col) in [
            (glam::Vec3::new(0.92, -0.3, -0.65), "H", 0xc6b3e8),
            (glam::Vec3::new(-0.9, -0.3, 0.70), "L", 0xaacfe5),
            (glam::Vec3::new(-0.9, 0.92, -0.65), "C", 0xe4c27e),
        ] {
            label(s.into(), project_world(v), col, window, cx);
        }
        if let Some(zero) = project(Oklch::new(0., 0., 0.)) {
            circle(zero, 4., rgb(0xc6b3e8).into(), None, window);
            label(
                "0°".into(),
                point(zero.x + px(6.), zero.y),
                0xc6b3e8,
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
                    Hsla::from(rgb(0xffffff)).alpha((0.7 / (1. + d * 8.)) as f32),
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
            (4.6, 0x10151c),
            (2., if outside { WARNING } else { 0xf3f7fc }),
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
                    0x77998b,
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
                            line(screen(node.position), pos, 1.2, ACCENT, window);
                            let mut square = fill(
                                Bounds::new(
                                    point(pos.x - px(4.), pos.y - px(4.)),
                                    size(px(8.), px(8.)),
                                ),
                                rgb(ACCENT),
                            );
                            square.border_widths = Edges::all(px(1.));
                            square.border_color = rgb(0x102018).into();
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
                        ACCENT
                    } else {
                        0x0f171d
                    })
                    .into(),
                    Some((2., rgb(ACCENT).into())),
                    window,
                );
                // Keep endpoint labels inside the viewport.
                let dx = if node.position[0] > 0.94 { -20. } else { 10. };
                let dy = if node.position[1] > 0.94 { 8. } else { -20. };
                label(
                    (i + 1).to_string(),
                    point(pos.x + px(dx), pos.y + px(dy)),
                    ACCENT,
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
        let colors = [0xc6b3e8, 0xaacfe5, 0xe4c27e];
        for s in intersections.points.iter().take(32) {
            if let Some(pos) = project(s.color) {
                circle(
                    pos,
                    5.,
                    rgb(colors[i]).into(),
                    Some((1.5, rgb(0x0a0d11).into())),
                    window,
                );
                label(
                    format!("{}={:.3}", p.parameter, s.t),
                    point(pos.x + px(8.), pos.y + px(5.)),
                    colors[i],
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
            rgb(0xffffff).into(),
            Some((2., rgb(0x172330).into())),
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
                    .child(tabs(2), None),
                None,
            )
            .child(
                DockLayout::h_split()
                    .child(tabs(3), None)
                    .child(tabs(4), None),
                None,
            );
        let left = tabs(0);
        let bottom = tabs(5);
        let right = tabs(6);
        self.dock.update(cx, |area, cx| {
            area.set_center(center, window, cx);
            area.set_dock(DockPlacement::Left, left, window, cx);
            area.set_dock_size(DockPlacement::Left, px(310.), window, cx);
            area.set_dock(DockPlacement::Bottom, bottom, window, cx);
            area.set_dock_size(DockPlacement::Bottom, px(184.), window, cx);
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
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_sm()
            .child(
                row()
                    .h(px(52.))
                    .px_5()
                    .gap_4()
                    .border_b_1()
                    .border_color(rgb(BORDER))
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
                                    .text_color(rgb(MUTED))
                                    .child("COLORMAP STUDIO"),
                            ),
                    )
                    .child(div().w(px(1.)).h_8().bg(rgb(BORDER)))
                    .child(caption(m.project.name.clone()).flex_1())
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
                    .border_color(rgb(BORDER))
                    .text_xs()
                    .child(
                        div()
                            .w(px(6.))
                            .h(px(6.))
                            .rounded_full()
                            .bg(rgb(if m.error.is_some() { WARNING } else { ACCENT })),
                    )
                    .child(
                        caption(if m.error.is_some() {
                            "Invalid expression · last valid path".into()
                        } else {
                            m.notice.clone()
                        })
                        .flex_1()
                        .truncate(),
                    )
                    .child(caption(format!(
                        "{CURVE_SAMPLES} path samples · {:.1}% outside sRGB",
                        outside as f64 / 256. * 100.
                    )))
                    .child(caption(format!("WGPU · {}", m.gpu.adapter_name))),
            )
    }
}
