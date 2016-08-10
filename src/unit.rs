use nalgebra::{Isometry2, Point2, Vector1, Vector2};
use ncollide::query::{self, Proximity};
use ncollide::shape::{ConvexHull, Cuboid};
use piston_window::*;
use std::collections::{HashSet};
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::str::FromStr;
use std::path::Path;
use regex::Regex;
use uuid::Uuid;

pub type Color = [f32; 4];
pub type Id = Uuid;
pub type Ids = HashSet<Id>;

pub const BLUE: Color = [0.0, 0.0, 1.0, 1.0];
pub const GREEN: Color = [0.0, 1.0, 0.0, 1.0];
pub const RED: Color = [1.0, 0.0, 0.0, 1.0];
pub const BLACK: Color = [0.0, 0.0, 0.0, 1.0];
pub const GRAY: Color = [0.0, 0.0, 0.0, 0.3];

#[derive(Clone, Debug, PartialEq)]
pub enum UnitRole {
    Soldier,
    General,
    Bullet,
}

impl ToString for UnitRole {
    fn to_string(&self) -> String {
        match *self {
            UnitRole::Soldier => "soldier".to_string(),
            UnitRole::General => "general".to_string(),
            UnitRole::Bullet => "bullet".to_string(),
        }
    }
}

impl FromStr for UnitRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "soldier" => Ok(UnitRole::Soldier),
            "general" => Ok(UnitRole::General),
            "bullet" => Ok(UnitRole::Bullet),
            _ => Err(s.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UnitState {
    Idle,
    Moving(f64, f64),
    Shooting(f64, f64),
    Dead,
}

impl ToString for UnitState {
    fn to_string(&self) -> String {
        match *self {
            UnitState::Moving(x, y) => format!("moving({:.*}, {:.*})", 2, x, 2, y),
            UnitState::Shooting(x, y) => format!("shooting({:.*}, {:.*})", 2, x, 2, y),
            UnitState::Dead => "dead".to_string(),
            UnitState::Idle => "idle".to_string(),
        }
    }
}

impl FromStr for UnitState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "dead" {
            return Ok(UnitState::Dead);
        }

        if s == "idle" {
            return Ok(UnitState::Idle);
        }

        let re = Regex::new(r"moving\((?P<x>\d+.\d+), (?P<y>\d+.\d+)\)").unwrap();
        if let Some(caps) = re.captures(s) {
            let x = f64::from_str(caps.name("x").unwrap()).unwrap();
            let y = f64::from_str(caps.name("y").unwrap()).unwrap();
            return Ok(UnitState::Moving(x, y));
        };

        let re = Regex::new(r"shooting\((?P<x>\d+.\d+), (?P<y>\d+.\d+)\)").unwrap();
        if let Some(caps) = re.captures(s) {
            let x = f64::from_str(caps.name("x").unwrap()).unwrap();
            let y = f64::from_str(caps.name("y").unwrap()).unwrap();
            return Ok(UnitState::Moving(x, y));
        };

        Err(s.to_string())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub id: Id,
    color: Color,
    pub x: f64,
    pub y: f64,
    width: f64,
    speed: f64,
    pub rotation: f64,
    shape: Cuboid<Vector2<f64>>,
    pub role: UnitRole,
    pub state: UnitState,
    pub on_collision: Option<String>,
    pub on_state_change: Option<String>,
    pub on_enter_view: Option<String>,
}

impl Unit {
    fn new(role: UnitRole, x: f64, y: f64, width: f64, speed: f64) -> Unit {
        let color = match role {
            UnitRole::Soldier => BLUE,
            UnitRole::General => RED,
            UnitRole::Bullet => BLACK,
        };

        Unit {
            id: Uuid::new_v4(),
            color: color,
            x: x,
            y: y,
            width: width,
            speed: speed,
            rotation: 0.0,
            shape: Cuboid::new(Vector2::new(width * 0.5, width * 0.5)),
            role: role,
            state: UnitState::Idle,
            on_collision: None,
            on_state_change: None,
            on_enter_view: None,
        }
    }

    pub fn new_general(x: f64, y: f64) -> Unit {
        let mut unit = Self::new(UnitRole::General, x, y, 50.0, 50.0);

        if let Ok(script) = Self::read_script("general", "on_collision") {
            unit.on_collision = Some(script);
        }
        if let Ok(script) = Self::read_script("general", "on_state_change") {
            unit.on_state_change = Some(script);
        }
        if let Ok(script) = Self::read_script("general", "on_enter_view") {
            unit.on_enter_view = Some(script);
        }
        unit
    }

    pub fn new_soldier(x: f64, y: f64) -> Unit {
        let mut unit = Self::new(UnitRole::Soldier, x, y, 25.0, 50.0);

        if let Ok(script) = Self::read_script("soldier", "on_collision") {
            unit.on_collision = Some(script);
        }
        if let Ok(script) = Self::read_script("soldier", "on_state_change") {
            unit.on_state_change = Some(script);
        }
        if let Ok(script) = Self::read_script("soldier", "on_enter_view") {
            unit.on_enter_view = Some(script);
        }
        unit
    }

    pub fn new_bullet(x: f64, y: f64) -> Unit {
        let mut unit = Self::new(UnitRole::Bullet, x, y, 5.0, 200.0);

        if let Ok(script) = Self::read_script("bullet", "on_collision") {
            unit.on_collision = Some(script);
        }
        if let Ok(script) = Self::read_script("bullet", "on_state_change") {
            unit.on_state_change = Some(script);
        }
        if let Ok(script) = Self::read_script("bullet", "on_enter_view") {
            unit.on_enter_view = Some(script);
        }
        unit
    }

    pub fn update(&mut self, args: &UpdateArgs) {
        match self.state {
            UnitState::Moving(x, y) => {
                let dist = self.speed * args.dt;

                if x < self.x + dist && x > self.x - dist && y < self.y + dist &&
                   y > self.y - dist {
                    self.x = x;
                    self.y = y;
                    self.state = UnitState::Idle;
                    return;
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

                self.x += xdelta * (xdist / (xdist + ydist));
                self.y += ydelta * (ydist / (xdist + ydist));
            }
            UnitState::Shooting(x, y) => unimplemented!(),
            UnitState::Idle | _ => {}
        }
    }

    pub fn render<G: Graphics>(&self, _: &RenderArgs, c: &Context, g: &mut G) {
        let transform = c.transform.trans(self.x, self.y).rot_rad(self.rotation);

        let half_width = self.width / 2.0;
        let square = rectangle::square(-half_width, -half_width, self.width);
        rectangle(self.color, square, transform, g);

        if self.role == UnitRole::Bullet {
            return;
        }

        let nose_width = self.width / 5.0;
        let nose = [-nose_width / 2.0, -half_width - nose_width / 2.0, nose_width, nose_width];
        rectangle(self.color, nose, transform, g);

        polygon(GRAY,
                &[[0.0, half_width], [-150.0, -150.0], [150.0, -150.0]],
                transform,
                g);
    }

    pub fn overlaps(&self, other: &Unit) -> bool {
        match query::proximity(&self.position(),
                               &self.shape,
                               &other.position(),
                               &other.shape,
                               0.0) {
            Proximity::Intersecting => true,
            Proximity::Disjoint | Proximity::WithinMargin => false,
        }
    }

    pub fn can_see(&self, other: &Unit) -> bool {
        match query::proximity(&self.position(),
                               &self.fov(),
                               &other.position(),
                               &other.shape,
                               0.0) {
            Proximity::Intersecting => true,
            Proximity::Disjoint | Proximity::WithinMargin => false,
        }
    }

    fn position(&self) -> Isometry2<f64> {
        Isometry2::new(Vector2::new(self.x, self.y), Vector1::new(self.rotation))
    }

    fn fov(&self) -> ConvexHull<Point2<f64>> {
        ConvexHull::new(vec![Point2::new(0.0, self.width * 0.5),
                             Point2::new(-150.0, -150.0),
                             Point2::new(150.0, -150.0)])
    }

    fn read_script(prefix: &str, suffix: &str) -> Result<String, io::Error> {
        let path_string = format!("lua/{}_{}.lua", prefix, suffix);
        let path = Path::new(&path_string);
        let mut file = try!(File::open(path));
        let mut s = String::new();
        try!(file.read_to_string(&mut s));
        Ok(s)
    }
}
