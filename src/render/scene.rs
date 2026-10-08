//! Draws a set of 3D figures on top of the dungeon: pose -> raycast -> ASCII shader.

use glam::Vec3;

use super::ascii_shader::{resolve, FigureStyle, ResolveCtx, ShaderParams};
use super::cell::CellGrid;
use super::figure::{pose, ClipPlay, Figure};
use super::iso::Camera;
use super::material::MaterialLib;
use super::raycast::{raycast_figure, SampleBuffer};
use super::world::{LightSrc, WorldLayer};

pub struct FigureInst<'a> {
    pub fig: &'a Figure,
    pub pos: Vec3,
    pub facing: f32,
    pub scale: f32,
    pub squash: Vec3,
    pub clips: Vec<ClipPlay<'a>>,
    pub style: FigureStyle,
}

impl<'a> FigureInst<'a> {
    pub fn new(fig: &'a Figure, pos: Vec3, facing: f32) -> FigureInst<'a> {
        FigureInst { fig, pos, facing, scale: 1.0, squash: Vec3::ONE, clips: Vec::new(), style: FigureStyle::default() }
    }
}

#[derive(Default)]
pub struct Scene {
    pub samples: SampleBuffer,
    styles: Vec<FigureStyle>,
}

impl Scene {
    pub fn draw(
        &mut self,
        grid: &mut CellGrid,
        layer: &WorldLayer,
        cam: &Camera,
        figs: &[FigureInst],
        lights: &[LightSrc],
        mats: &MaterialLib,
        params: &ShaderParams,
    ) {
        self.samples.begin(grid.w, grid.h);
        self.styles.clear();
        for (i, f) in figs.iter().enumerate() {
            let p = pose(f.fig, f.pos, f.facing, f.scale, f.squash, &f.clips);
            raycast_figure(&mut self.samples, cam, f.fig, &p, (i + 1) as u16);
            self.styles.push(f.style);
        }
        let rc = ResolveCtx { layer, mats, lights, params, styles: &self.styles };
        resolve(grid, &mut self.samples, &rc);
    }
}
