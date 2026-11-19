use okclrmp_core::{
    Colormap, LoadError, Oklch, Project, SampleError,
    constants::{Constant, ConstantKind, SliderBinding},
    model::{Mode, Plane},
};
use std::{
    io::Cursor,
    sync::atomic::{AtomicU64, Ordering},
};

// A pre-cubic project, with the historical constants string and no newer flags.
const LEGACY: &str = r#"{
    "name": "Legacy test", "parameter": "t", "constants": "hue=140",
    "expressions": ["t", "0.6*t*(1-t)", "hue"],
    "mode": {"Spline": "Cl"}, "nodes": [[0,0],[0.5,0.15],[1,0]],
    "imported": [], "slices": [0.65,0.1,140], "cursor": 0.5,
    "sample_count": 256, "map_chroma": true,
    "yaw": -0.65, "pitch": 0.65, "zoom": 1
}"#;

#[test]
fn legacy_json_reader_and_serde_preserve_the_original_curve() {
    let maps = [
        Colormap::from_json(LEGACY).unwrap(),
        Colormap::from_reader(Cursor::new(LEGACY.as_bytes())).unwrap(),
        serde_json::from_str::<Colormap>(LEGACY).unwrap(),
    ];
    for map in maps {
        for i in 0..=32 {
            let t = i as f64 / 32.;
            let color = map.sample(t).unwrap();
            assert!((color.l - t).abs() < 1e-12);
            assert!((color.c - 0.6 * t * (1. - t)).abs() < 1e-12);
            assert_eq!(color.h, 140.);
        }
        assert_eq!(map.sample(-2.).unwrap(), map.sample(0.).unwrap());
        assert_eq!(map.sample(2.).unwrap(), map.sample(1.).unwrap());
    }
}

#[test]
fn expressions_constants_and_variables_are_compiled_once_and_thread_safe() {
    let mut p = Project::preset(2);
    p.parameter = "v".into();
    p.definitions = "bias = 2 * abs(v - 0.5)".into();
    p.expressions = [
        "1-0.36*bias".into(),
        "c_max*bias/2".into(),
        "v < 0.5 ? 15 : 263".into(),
    ];
    let map = Colormap::from_project(&p).unwrap();
    // Mutating the editor project after compilation cannot affect the map.
    p.constants[0].value = 0.1;
    for t in [0., 0.12345, 0.5, 0.87654, 1.] {
        let c = map.sample(t).unwrap();
        let bias = 2. * (t - 0.5).abs();
        assert_eq!(
            c,
            Oklch::new(
                1. - 0.36 * bias,
                0.3 * bias / 2.,
                if t < 0.5 { 15. } else { 263. }
            )
        );
    }
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<Colormap>();
    let expected = map.sample_rgba8(0.25).unwrap();
    assert_eq!(
        std::thread::spawn(move || map.sample_rgba8(0.25).unwrap())
            .join()
            .unwrap(),
        expected
    );
}

#[test]
fn spline_constants_and_named_slider_bindings_follow_the_app_load_rules() {
    for plane in Plane::ALL {
        let mut p = Project::preset(1);
        p.enable_spline(plane, &p.compile().unwrap());
        p.spline_constant = true;
        let axis = plane.axes().2;
        let value = [0.71, 0.18, 359.5][axis];
        p.constants.push(Constant {
            name: "bound".into(),
            value,
            kind: if axis == 2 {
                ConstantKind::Degree
            } else {
                ConstantKind::Fraction
            },
            binding: Some(SliderBinding::ALL[axis]),
        });
        // A persisted slider can disagree; named constants supply it on load.
        p.slices[axis] = 0.;
        let json = serde_json::to_string(&p).unwrap();
        let map = Colormap::from_json(&json).unwrap();
        let mut loaded: Project = serde_json::from_str(&json).unwrap();
        loaded.migrate_spline().unwrap();
        loaded.sync_bound_slices().unwrap();
        let expected = loaded.compile().unwrap();
        for t in [0., 0.123, 0.5, 0.987, 1.] {
            assert_eq!(map.sample(t).unwrap(), expected.at(t));
            assert_eq!(map.sample(t).unwrap().channel(axis), value);
        }
        // Loading the map does not rewrite the caller's document.
        let unsynced: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(unsynced.slices[axis], 0.);
    }
}

#[test]
fn imported_paths_interpolate_across_the_hue_seam_and_honor_inversion() {
    let mut p = Project {
        mode: Mode::Imported,
        nodes: vec![],
        spline: None,
        imported: vec![Oklch::new(0.2, 0.05, 350.), Oklch::new(0.8, 0.15, 10.)],
        ..Project::default()
    };
    let forward = Colormap::from_project(&p).unwrap();
    assert!((forward.sample(0.5).unwrap().h - 360.).abs() < 1e-12);
    p.reverse_output = true;
    let reversed = Colormap::from_project(&p).unwrap();
    for t in [0., 0.25, 0.5, 0.75, 1.] {
        assert_eq!(reversed.sample(t).unwrap(), forward.sample(1. - t).unwrap());
    }
    assert_eq!(reversed.sample(-10.).unwrap(), forward.sample(1.).unwrap());
    assert_eq!(reversed.sample(10.).unwrap(), forward.sample(0.).unwrap());
}

#[test]
fn rgb_conversion_and_lut_order_follow_saved_output_settings() {
    let mut p = Project::preset(1);
    p.expressions = ["0.4+0.3*t".into(), "0.5".into(), "45".into()];
    for mapping in [false, true] {
        p.map_chroma = mapping;
        p.reverse_output = false;
        let forward = Colormap::from_project(&p).unwrap();
        for t in [0., 0.23, 0.5, 1.] {
            let expected = Oklch::new(0.4 + 0.3 * t, 0.5, 45.);
            assert_eq!(forward.sample(t).unwrap(), expected);
            assert_eq!(forward.sample_srgb(t).unwrap(), expected.srgb(mapping));
            assert_eq!(forward.sample_rgba8(t).unwrap(), expected.rgba8(mapping));
        }
        let lut = forward.rgba8_lut(257).unwrap();
        assert_eq!(lut.len(), 257);
        assert_eq!(lut[0], forward.sample_rgba8(0.).unwrap());
        assert_eq!(lut[256], forward.sample_rgba8(1.).unwrap());
        p.reverse_output = true;
        let reversed = Colormap::from_project(&p).unwrap();
        assert_eq!(
            reversed.rgba8_lut(257).unwrap(),
            lut.into_iter().rev().collect::<Vec<_>>()
        );
        assert_eq!(reversed.rgba8_lut(4097).unwrap().len(), 4097);
        assert_eq!(forward.rgba8_lut(0), Err(SampleError::InvalidSampleCount));
        assert_eq!(forward.rgba8_lut(1), Err(SampleError::InvalidSampleCount));
    }
    p.map_chroma = true;
    let mapped = Colormap::from_project(&p)
        .unwrap()
        .sample_rgba8(0.5)
        .unwrap();
    p.map_chroma = false;
    assert_ne!(
        mapped,
        Colormap::from_project(&p)
            .unwrap()
            .sample_rgba8(0.5)
            .unwrap()
    );
}

#[test]
fn invalid_documents_parameters_and_unsampled_singularities_return_errors() {
    assert!(matches!(Colormap::from_json("{"), Err(LoadError::Json(_))));
    assert!(matches!(Colormap::from_json("{}"), Err(LoadError::Json(_))));
    let mut p = Project::preset(1);
    p.expressions[0] = "unknown_variable".into();
    assert!(matches!(
        Colormap::from_json(&serde_json::to_string(&p).unwrap()),
        Err(LoadError::InvalidProject(_))
    ));
    assert!(serde_json::from_str::<Colormap>(&serde_json::to_string(&p).unwrap()).is_err());
    // The load-time grid cannot detect every singularity; sampling must check.
    p.expressions[0] = "1/(t-0.12345)".into();
    let map = Colormap::from_project(&p).unwrap();
    assert_eq!(
        map.sample(0.12345),
        Err(SampleError::UndefinedColor { t: 0.12345 })
    );
    for t in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(map.sample(t), Err(SampleError::NonFiniteParameter));
        assert_eq!(map.sample_srgb(t), Err(SampleError::NonFiniteParameter));
        assert_eq!(map.sample_rgba8(t), Err(SampleError::NonFiniteParameter));
    }
    // Finite OKLCH values can overflow RGB math; never return NaN RGB or black.
    p.expressions[0] = "1e300".into();
    p.map_chroma = false;
    let map = Colormap::from_project(&p).unwrap();
    assert!(map.sample(0.).is_ok());
    assert_eq!(
        map.sample_srgb(0.),
        Err(SampleError::UndefinedColor { t: 0. })
    );
    assert_eq!(map.rgba8_lut(2), Err(SampleError::UndefinedColor { t: 0. }));
}

#[test]
fn loads_a_real_file_and_reports_io_failures() {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "okclrmp-core-{}-{}.json",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, LEGACY).unwrap();
    let result = Colormap::load(&path);
    std::fs::remove_file(&path).unwrap();
    let color = result.unwrap().sample(0.5).unwrap();
    assert_eq!((color.l, color.h), (0.5, 140.));
    assert!((color.c - 0.15).abs() < 1e-12);
    assert!(
        matches!(Colormap::load(&path),Err(LoadError::Io(e)) if e.kind()==std::io::ErrorKind::NotFound)
    );
}
