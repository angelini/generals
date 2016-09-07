use nalgebra::{Isometry2, Vector1, Vector2};
use std::f64;

pub const SCENE_SIZE: [u32; 2] = [800, 800];
const PI: f64 = f64::consts::PI;
const TWO_PI: f64 = f64::consts::PI * 2.0;
const HALF_PI: f64 = f64::consts::PI * 0.5;

#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

impl Pose {
    pub fn new(x: f64, y: f64, rotation: f64) -> Pose {
        Pose {
            x: x,
            y: y,
            rotation: rotation,
        }
    }

    pub fn move_towards(&self, x: f64, y: f64, dist: f64) -> Pose {
        if x < self.x + dist && x > self.x - dist && y < self.y + dist && y > self.y - dist {
            return Pose::new(x, y, self.rotation);
        }

        let (xdist, xdelta) = if x > self.x {
            (x - self.x, dist)
        } else if x < self.x {
            (self.x - x, -dist)
        } else {
            (0.0, 0.0)
        };

        let (ydist, ydelta) = if y > self.y {
            (y - self.y, dist)
        } else if y < self.y {
            (self.y - y, -dist)
        } else {
            (0.0, 0.0)
        };

        Pose::new(self.x + xdelta * (xdist / (xdist + ydist)),
                  self.y + ydelta * (ydist / (xdist + ydist)),
                  self.rotation)
    }

    pub fn rotate_towards(&self, x: f64, y: f64, dt: f64) -> Pose {
        let dx = x - self.x;
        let dy = y - self.y;

        let mut dest_rotation = (dy.atan2(dx) + HALF_PI) % TWO_PI;
        let curr_rotation = self.rotation % (TWO_PI);

        if dest_rotation < 0.0 {
            dest_rotation += TWO_PI;
        }

        let delta = dest_rotation - curr_rotation;

        let rotation = if dest_rotation <= curr_rotation + dt &&
                          dest_rotation >= curr_rotation - dt {
            dest_rotation
        } else if delta > 0.0 && delta < PI {
            self.rotation + dt
        } else if self.rotation - dt < 0.0 {
            self.rotation - dt + TWO_PI
        } else {
            self.rotation - dt
        };

        Pose::new(self.x, self.y, rotation)
    }

    pub fn isometry(&self) -> Isometry2<f64> {
        Isometry2::new(Vector2::new(self.x, self.y), Vector1::new(self.rotation))
    }

    pub fn render_pose(&self) -> (f64, f64, f64) {
        (self.x, SCENE_SIZE[1] as f64 - self.y, -self.rotation)
    }
}
