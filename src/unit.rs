use nalgebra::{Point2, Vector2};
use ncollide::query::{self, PointQuery, Proximity};
use ncollide::shape::{ConvexHull, Cuboid};
use piston_window::*;
use std::collections::{HashMap, HashSet};
use std::f64;
use std::str::FromStr;
use uuid::Uuid;

use geometry::Pose;
use parser::{self, TokenType};

pub type Color = [f32; 4];
pub type Id = Uuid;
pub type Ids = HashSet<Id>;

pub type UnitShape = Cuboid<Vector2<f64>>;

pub const BLUE: Color = [0.0, 0.0, 1.0, 1.0];
pub const PURPLE: Color = [0.5, 0.5, 1.0, 1.0];
pub const GREEN: Color = [0.0, 1.0, 0.0, 1.0];
pub const RED: Color = [1.0, 0.0, 0.0, 1.0];
pub const BLACK: Color = [0.0, 0.0, 0.0, 1.0];
pub const GRAY: Color = [0.0, 0.0, 0.0, 0.3];
pub const LIGHT_GRAY: Color = [0.0, 0.0, 0.0, 0.1];

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
    pub pose: Pose,
    width: f64,
    speed: f64,
    pub shape: UnitShape,
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
                (25.0, 100.0, color)
            }
            UnitRole::General => (50.0, 50.0, RED),
            UnitRole::Bullet => (5.0, 150.0, BLACK),
        };

        Unit {
            id: id,
            team: team,
            color: color,
            pose: Pose::new(x, y, rotation),
            width: width,
            speed: speed,
            shape: UnitShape::new(Vector2::new(width * 0.5, width * 0.5)),
            role: role,
            state: state,
            state_queue: Vec::new(),
        }
    }

    #[allow(float_cmp)]
    pub fn update(&mut self, args: &UpdateArgs, views: &HashMap<Id, (Pose, UnitShape)>) -> Vec<Unit> {
        match self.state {
            UnitState::Move(x, y) => {
                self.pose = self.pose.move_towards(x, y, self.speed * args.dt);

                if self.pose.x == x && self.pose.y == y {
                    let original_state = self.state;
                    self.state = self.next_state();
                    info!(target: "units",
                        "{:?} {:?} -> {:?}", self.role, original_state, self.state);
                }

                vec![]
            }
            UnitState::Look(x, y) => {
                self.pose = self.pose
                    .rotate_towards(x, y, 1.2 * args.dt)
                    .move_towards(x, y, self.speed * args.dt);

                if self.can_see_point(x, y) {
                    let original_state = self.state;
                    self.state = self.next_state();
                    info!(target: "units",
                          "{:?} {:?} -> {:?}", self.role, original_state, self.state);
                }

                vec![]
            }
            UnitState::Shoot(id) => {
                let &(pose, ref shape) = match views.get(&id) {
                    Some(tuple) => tuple,
                    None => {
                        self.state = self.next_state();
                        return vec![];
                    }
                };

                if self.can_shoot(&pose, shape) {
                    let bullet_pose = self.pose.move_towards(pose.x, pose.y, self.width);
                    self.state = self.next_state();
                    vec![Unit::new(
                        UnitRole::Bullet,
                        Id::new_v4(),
                        bullet_pose.x,
                        bullet_pose.y,
                        bullet_pose.rotation,
                        self.team,
                        UnitState::Move(pose.x, pose.y))]
                } else {
                    self.pose = self.pose
                        .rotate_towards(pose.x, pose.y, 1.2 * args.dt)
                        .move_towards(pose.x, pose.y, self.speed * args.dt);
                    vec![]
                }
            }
            UnitState::Idle | _ => vec![],
        }
    }

    pub fn render<G: Graphics>(&self, _: &RenderArgs, c: &Context, g: &mut G) {
        let (x, y, rotation) = self.pose.render_pose();
        let transform = c.transform.trans(x, y).rot_rad(rotation);

        let half_width = self.width / 2.0;
        let square = rectangle::square(-half_width, -half_width, self.width);
        rectangle(self.color, square, transform, g);

        if self.role == UnitRole::Bullet {
            return;
        }

        let nose_width = self.width / 5.0;
        let nose = [half_width, -nose_width / 2.0, nose_width, nose_width];
        rectangle(self.color, nose, transform, g);

        polygon(LIGHT_GRAY,
                &[[0.0, 0.0], [150.0, 150.0], [150.0, -150.0]],
                transform,
                g);

        polygon(GRAY,
                &[[0.0, 0.0], [120.0, 60.0], [120.0, -60.0]],
                transform,
                g);
    }

    pub fn overlaps(&self, other: &Unit) -> bool {
        match query::proximity(&self.pose.isometry(),
                               &self.shape,
                               &other.pose.isometry(),
                               &other.shape,
                               0.0) {
            Proximity::Intersecting => true,
            Proximity::Disjoint | Proximity::WithinMargin => false,
        }
    }

    pub fn can_see(&self, other: &Unit) -> bool {
        match query::proximity(&self.pose.isometry(),
                               &self.fov(),
                               &other.pose.isometry(),
                               &other.shape,
                               0.0) {
            Proximity::Intersecting => true,
            Proximity::Disjoint | Proximity::WithinMargin => false,
        }
    }

    fn can_shoot(&self, pose: &Pose, shape: &UnitShape) -> bool {
        match query::proximity(&self.pose.isometry(),
                               &self.range(),
                               &pose.isometry(),
                               shape,
                               0.0) {
            Proximity::Intersecting => true,
            Proximity::Disjoint | Proximity::WithinMargin => false,
        }
    }

    pub fn xy(&self) -> (f64, f64) {
        (self.pose.x, self.pose.y)
    }

    fn can_see_point(&self, x: f64, y: f64) -> bool {
        self.fov().contains_point(&self.pose.isometry(), &Point2::new(x, y))
    }

    fn fov(&self) -> ConvexHull<Point2<f64>> {
        ConvexHull::new(vec![Point2::new(0.0, 0.0),
                             Point2::new(150.0, 150.0),
                             Point2::new(150.0, -150.0)])
    }

    fn range(&self) -> ConvexHull<Point2<f64>> {
        ConvexHull::new(vec![Point2::new(0.0, 0.0),
                             Point2::new(120.0, 60.0),
                             Point2::new(120.0, -60.0)])
    }

    fn next_state(&mut self) -> UnitState {
        self.state_queue.pop().unwrap_or(UnitState::Idle)
    }
}
