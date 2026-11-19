//! The headless API must reproduce the app's saved sample curves and RGB exports.
use okclrmp::{
    Colormap,
    exchange::{self, ExportSpace},
    model::Project,
};

#[test]
fn public_core_api_matches_all_sample_projects_and_the_app_exporter() {
    for source in [
        include_str!("../samples/linear.green.json"),
        include_str!("../samples/linear.viridis.json"),
        include_str!("../samples/symm.greenmagenta.json"),
        include_str!("../samples/symm.turbo.json"),
        include_str!("../samples/cyclic.hue.json"),
    ] {
        let mut project: Project = serde_json::from_str(source).unwrap();
        project.migrate_spline().unwrap();
        project.sync_bound_slices().unwrap();
        for reverse in [false, true] {
            for map_chroma in [false, true] {
                project.reverse_output = reverse;
                project.map_chroma = map_chroma;
                let json = serde_json::to_string(&project).unwrap();
                let map = Colormap::from_json(&json).unwrap();
                // Match the app opening the very same saved document, including
                // JSON float parsing and its load-time binding synchronization.
                let mut loaded: Project = serde_json::from_str(&json).unwrap();
                loaded.migrate_spline().unwrap();
                loaded.sync_bound_slices().unwrap();
                let curve = loaded.compile().unwrap();
                for t in [0., 0.12345, 0.5, 0.98765, 1.] {
                    let expected = curve.at(if reverse { 1. - t } else { t });
                    assert_eq!(map.sample(t).unwrap(), expected);
                    assert_eq!(map.sample_rgba8(t).unwrap(), expected.rgba8(map_chroma));
                }
                let actual = map
                    .rgba8_lut(67)
                    .unwrap()
                    .iter()
                    .map(|[r, g, b, _]| format!("#{r:02x}{g:02x}{b:02x}"))
                    .collect::<Vec<_>>();
                let expected: Vec<String> = serde_json::from_str(&exchange::export_ordered(
                    &curve,
                    67,
                    ExportSpace::Hex,
                    map_chroma,
                    reverse,
                ))
                .unwrap();
                assert_eq!(actual, expected, "{}", project.name);
            }
        }
    }
}
