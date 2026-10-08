//! Snapshot tests of the ASCII shader: each primitive shape at a few angles, plus the
//! "x-ray" case (a figure hidden behind a wall). Review changes with `cargo insta review`.

use glam::Vec3;
use glyphcrawl::render::ascii_shader::{FigureStyle, ShaderParams};
use glyphcrawl::render::cell::{CellGrid, Rgb};
use glyphcrawl::render::figure::{Figure, FigureFile, PartDef, Shape};
use glyphcrawl::render::iso::{world_to_screen, Camera};
use glyphcrawl::render::material::MaterialLib;
use glyphcrawl::render::scene::{FigureInst, Scene};
use glyphcrawl::render::stamp::Face;
use glyphcrawl::render::world::{LightSrc, WorldLayer};

fn mats() -> MaterialLib {
    MaterialLib::parse(r##"{ "m": (color: "#c0c0c0"), "n": (color: "#c04040") }"##).unwrap()
}

fn figure(shape: Shape, size: (f32, f32, f32)) -> Figure {
    let f = FigureFile {
        scale: 1.0,
        parts: vec![PartDef {
            name: "p".into(),
            parent: None,
            shape,
            size,
            at: (0.0, size.1 / 2.0, 0.0),
            center: (0.0, 0.0, 0.0),
            rot: (0.0, 0.0, 0.0),
            mat: "m".into(),
            hidden: false,
        }],
        clips: Default::default(),
    };
    let lib = mats();
    Figure::from_file("t", f, |m| lib.id(m)).unwrap()
}

fn render(fig: &Figure, facing: f32, layer_fn: impl Fn(&mut WorldLayer)) -> String {
    let (w, h) = (24u16, 14u16);
    let mut grid = CellGrid::new(w, h);
    let mut layer = WorldLayer::default();
    layer.reset(w, h);
    layer_fn(&mut layer);
    let s0 = world_to_screen(Vec3::ZERO);
    let cam = Camera { col: (s0.x - 12.0) as i32, row: (s0.y - 11.0) as i32 };
    let mut f = FigureInst::new(fig, Vec3::ZERO, facing);
    f.style = FigureStyle { xray: Some(Rgb(0, 255, 255)), ..Default::default() };
    let lights = [LightSrc { pos: Vec3::new(2.0, 3.0, 2.0), color: [1.0, 1.0, 1.0], radius: 10.0, mask: None }];
    let mut scene = Scene::default();
    scene.draw(&mut grid, &layer, &cam, &[f], &lights, &mats(), &ShaderParams::default());
    grid.to_text()
}

#[test]
fn primitives_at_four_angles() {
    let shapes = [
        ("box", Shape::Box, (1.2, 1.4, 1.2)),
        ("sphere", Shape::Sphere, (1.4, 1.4, 1.4)),
        ("cylinder", Shape::Cylinder, (1.0, 1.6, 1.0)),
        ("cone", Shape::Cone, (1.2, 1.6, 1.2)),
        ("octahedron", Shape::Octahedron, (1.4, 1.6, 1.4)),
    ];
    for (name, shape, size) in shapes {
        let fig = figure(shape, size);
        let mut out = String::new();
        for deg in [0.0f32, 30.0, 45.0, 90.0] {
            out.push_str(&format!("--- {deg}°\n"));
            out.push_str(&render(&fig, deg.to_radians(), |_| {}));
        }
        insta::assert_snapshot!(name, out);
    }
}

#[test]
fn hidden_figure_draws_xray_outline() {
    let fig = figure(Shape::Box, (1.0, 1.6, 1.0));
    // A "wall" covering the whole lower half of the screen, taller than the figure.
    let txt = render(&fig, 0.0, |layer| {
        for i in 0..layer.height.len() {
            layer.height[i] = 3.0;
            layer.face[i] = Face::Top;
        }
    });
    insta::assert_snapshot!("xray", txt);
    // The outline must be there (a hidden box is not just empty space).
    assert!(txt.chars().any(|c| "|/\\_-'.".contains(c)));
}
