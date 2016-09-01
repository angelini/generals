use nalgebra::{Isometry2, Point2, Vector1, Vector2};
use ncollide::query::{self, PointQuery, Proximity};
use ncollide::shape::{ConvexHull, Cuboid};
use piston_window::*;
use std::collections::{HashMap, HashSet};
use std::f64;
use std::str::FromStr;
use uuid::Uuid;

use parser::{self, TokenType};

pub type Color = [f32; 4];
pub type Id = Uuid;
pub type Ids = HashSet<Id>;

pub const BLUE: Color = [0.0, 0.0, 1.0, 1.0];
pub const PURPLE: Color = [0.5, 0.5, 1.0, 1.0];
pub const GREEN: Color = [0.0, 1.0, 0.0, 1.0];
pub const RED: Color = [1.0, 0.0, 0.0, 1.0];
pub const BLACK: Color = [0.0, 0.0, 0.0, 1.0];
pub const GRAY: Color = [0.0, 0.0, 0.0, 0.3];

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnitState {
    Idle,
    Move(f64, f64),
    Look(f64, f64),
    Shoot(Id),
    Dead,
}

impl ToString for UnitState {
    fn to_string(&self) -> String {
        match *self {
            UnitState::Move(x, y) => format!("move({:.*}, {:.*})", 2, x, 2, y),
            UnitState::Look(x, y) => format!("look({:.*}, {:.*})", 2, x, 2, y),
            UnitState::Shoot(id) => format!("shoot({})", id),
            UnitState::Dead => "dead".to_string(),
            UnitState::Idle => "idle".to_string(),
        }
    }
}

impl FromStr for UnitState {
    type Err = parser::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parser::read_symbol(s) {
            Ok(("dead", _)) => return Ok(UnitState::Dead),
            Ok(("idle", _)) => return Ok(UnitState::Idle),
            _ => {}
        }

        match parser::read_fn(s) {
            Ok(("move", s)) => {
                let (x, s) = try!(parser::read_float(s));
                let (y, _) = try!(parser::read_float(s));
                Ok(UnitState::Move(x, y))
            }
            Ok(("look", s)) => {
                let (x, s) = try!(parser::read_float(s));
                let (y, _) = try!(parser::read_float(s));
                Ok(UnitState::Look(x, y))
            }
            Ok(("shoot", s)) => {
                let (id, _) = try!(parser::read_id(s));
                Ok(UnitState::Shoot(id))
            }
            _ => Err((String::from(s), TokenType::Other)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub id: Id,
    pub team: usize,
    color: Color,
    pub x: f64,
    pub y: f64,
    width: f64,
    speed: f64,
    pub rotation: f64,
    shape: Cuboid<Vector2<f64>>,
    pub role: UnitRole,
    pub state: UnitState,
    state_queue: Vec<UnitState>,
}

impl Unit {
    pub fn new(role: UnitRole,
               id: Id,
               x: f64,
               y: f64,
               rotation: f64,
               team: usize,
               state: UnitState)
               -> Unit {
        let (width, speed, color) = match role {
            UnitRole::Soldier => {
                let color = if team == 1 {
                    BLUE
                } else {
                    PURPLE
                };
                (25.0, 150.0, color)
            }
            UnitRole::General => (50.0, 50.0, RED),
            UnitRole::Bullet => (5.0, 150.0, BLACK),
        };

        Unit {
            id: id,
            team: team,
            color: color,
            x: x,
            y: y,
            width: width,
            speed: speed,
            rotation: rotation,
            shape: Cuboid::new(Vector2::new(width * 0.5, width * 0.5)),
            role: role,
            state: state,
            state_queue: Vec::new(),
        }
    }

    pub fn update(&mut self, args: &UpdateArgs, views: &HashMap<Id, (f64, f64)>) -> Vec<Unit> {
        match self.state {
            UnitState::Move(x, y) => {
                let moved = self.move_self_towards(x, y, args.dt);

                if moved {
                    let original_state = self.state;
                    self.state = self.next_state();
                    info!(target: "units",
                        "{:?} {:?} -> {:?}", self.role, original_state, self.state);
                }
                vec![]
            }
            UnitState::Look(x, y) => {
                let rotated = self.rotate_self_towards(x, y, args.dt);

                if rotated {
                    if self.can_see_point(x, y) {
                        let original_state = self.state;
                        self.state = self.next_state();
                        info!(target: "units",
                            "{:?} {:?} -> {:?}", self.role, original_state, self.state);
                    } else {
                        self.move_self_towards(x, y, args.dt);
                    }
                }
                vec![]
            }
            UnitState::Shoot(id) => {
                let &(x, y) = match views.get(&id) {
                    Some(xy) => xy,
                    None => {
                        self.state = self.next_state();
                        return vec![];
                    }
                };
                let rotated = self.rotate_self_towards(x, y, args.dt);

                if rotated {
                    if self.can_see_point(x, y) {
                        let (xdelta, ydelta) = self.move_towards(x, y, self.width + 10.0);
                        self.state = self.next_state();
                        vec![Unit::new(
                            UnitRole::Bullet,
                            Id::new_v4(),
                            self.x + xdelta,
                            self.y + ydelta,
                            0.0,
                            self.team,
                            UnitState::Move(x, y))]
                    } else {
                        self.move_self_towards(x, y, args.dt);
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            UnitState::Idle | _ => vec![],
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

    pub fn xy(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    fn can_see_point(&self, x: f64, y: f64) -> bool {
        self.fov().contains_point(&self.position(), &Point2::new(x, y))
    }

    fn position(&self) -> Isometry2<f64> {
        Isometry2::new(Vector2::new(self.x, self.y), Vector1::new(self.rotation))
    }

    fn fov(&self) -> ConvexHull<Point2<f64>> {
        ConvexHull::new(vec![Point2::new(0.0, self.width * 0.5),
                             Point2::new(-150.0, -150.0),
                             Point2::new(150.0, -150.0)])
    }

    fn next_state(&mut self) -> UnitState {
        self.state_queue.pop().unwrap_or(UnitState::Idle)
    }

    fn move_self_towards(&mut self, x: f64, y: f64, dt: f64) -> bool {
        let dist = self.speed * dt;

        if x < self.x + dist && x > self.x - dist && y < self.y + dist && y > self.y - dist {
            self.x = x;
            self.y = y;
            true
        } else {
            let (xdelta, ydelta) = self.move_towards(x, y, dist);
            self.x += xdelta;
            self.y += ydelta;
            false
        }
    }

    fn move_towards(&self, x: f64, y: f64, dist: f64) -> (f64, f64) {
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

        (xdelta * (xdist / (xdist + ydist)), ydelta * (ydist / (xdist + ydist)))
    }

    fn rotate_self_towards(&mut self, x: f64, y: f64, dt: f64) -> bool {
        let dx = x - self.x;
        let dy = y - self.y;

        let mut dest_rotation = (dy.atan2(dx) + 0.5 * f64::consts::PI) % (2.0 * f64::consts::PI);
        let curr_rotation = self.rotation % (2.0 * f64::consts::PI);

        if dest_rotation < 0.0 {
            dest_rotation += 2.0 * f64::consts::PI;
        }

        if dest_rotation <= curr_rotation + dt && dest_rotation >= curr_rotation - dt {
            self.rotation = dest_rotation;
            true
        } else {

            let delta = dest_rotation - curr_rotation;
            if delta > 0.0 && delta < f64::consts::PI {
                self.rotation += dt;
            } else {
                self.rotation -= dt;
                if self.rotation < 0.0 {
                    self.rotation += 2.0 * f64::consts::PI;
                }
            }
            false
        }
    }
}
