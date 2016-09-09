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
pub type Views = HashMap<Id, (Pose, UnitShape)>;

pub const BLUE: Color = [0.0, 0.0, 1.0, 1.0];
pub const PURPLE: Color = [0.5, 0.5, 1.0, 1.0];
pub const GREEN: Color = [0.0, 1.0, 0.0, 1.0];
pub const RED: Color = [1.0, 0.0, 0.0, 1.0];
pub const BLACK: Color = [0.0, 0.0, 0.0, 1.0];
pub const GRAY: Color = [0.0, 0.0, 0.0, 0.3];
pub const LIGHT_GRAY: Color = [0.0, 0.0, 0.0, 0.1];

const FOV_POINTS: [[f64; 2]; 3] = [[0.0, 0.0], [200.0, 150.0], [200.0, -150.0]];
const RANGE_POINTS: [[f64; 2]; 3] = [[0.0, 0.0], [120.0, 20.0], [120.0, -20.0]];

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

#[derive(Clone, Debug, PartialEq)]
pub enum UnitState {
    Command(Id, Box<UnitState>),
    Dead,
    Idle,
    Look(f64, f64),
    Move(f64, f64),
    Shoot(Id),
}

const IDLE: &'static UnitState = &UnitState::Idle;

impl ToString for UnitState {
    fn to_string(&self) -> String {
        match *self {
            UnitState::Command(id, ref state) => format!("command({}, {})", id, state.to_string()),
            UnitState::Dead => "dead".to_string(),
            UnitState::Idle => "idle".to_string(),
            UnitState::Look(x, y) => format!("look({:.*}, {:.*})", 2, x, 2, y),
            UnitState::Move(x, y) => format!("move({:.*}, {:.*})", 2, x, 2, y),
            UnitState::Shoot(id) => format!("shoot({})", id),
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
            Ok(("command", s)) => {
                let (id, s) = try!(parser::read_id(s));
                let state = UnitState::from_str(s).unwrap();
                Ok(UnitState::Command(id, Box::new(state)))
            }
            Ok(("look", s)) => {
                let (x, s) = try!(parser::read_float(s));
                let (y, _) = try!(parser::read_float(s));
                Ok(UnitState::Look(x, y))
            }
            Ok(("move", s)) => {
                let (x, s) = try!(parser::read_float(s));
                let (y, _) = try!(parser::read_float(s));
                Ok(UnitState::Move(x, y))
            }
            Ok(("shoot", s)) => {
                let (id, _) = try!(parser::read_id(s));
                Ok(UnitState::Shoot(id))
            }
            _ => Err((String::from(s), TokenType::Other)),
        }
    }
}

pub struct UpdateResults {
    pub unit: Option<Unit>,
    pub command: Option<(Id, UnitState)>,
}

impl UpdateResults {
    fn empty() -> UpdateResults {
        UpdateResults {
            unit: None,
            command: None,
        }
    }

    fn from_unit(unit: Unit) -> UpdateResults {
        UpdateResults {
            unit: Some(unit),
            command: None,
        }
    }

    fn from_command(id: Id, state: UnitState) -> UpdateResults {
        UpdateResults {
            unit: None,
            command: Some((id, state)),
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
            UnitRole::General => (50.0, 100.0, RED),
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

    pub fn update(&mut self, args: &UpdateArgs, views: &Views) -> UpdateResults {
        let (pose, update_state, results) = match self.state {
            UnitState::Command(id, ref state) => self.update_command(id, state, args.dt, views),
            UnitState::Look(x, y) => {
                let (pose, update_state) = self.update_look(x, y, args.dt);
                (pose, update_state, UpdateResults::empty())
            }
            UnitState::Move(x, y) => {
                let (pose, update_state) = self.update_move(x, y, args.dt);
                (pose, update_state, UpdateResults::empty())
            }
            UnitState::Shoot(id) => self.update_shoot(id, args.dt, views),
            UnitState::Idle | _ => return UpdateResults::empty(),
        };

        self.pose = pose;

        if update_state {
            info!(target: "units",
                  "{:?} {:?} -> {:?}", self.role, self.state, &self.peek_next_state());
            self.state = self.next_state()
        }

        results
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

        polygon(LIGHT_GRAY, &FOV_POINTS, transform, g);
        polygon(GRAY, &RANGE_POINTS, transform, g);
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

    pub fn xy(&self) -> (f64, f64) {
        (self.pose.x, self.pose.y)
    }

    fn update_command(&self,
                      id: Id,
                      state: &UnitState,
                      dt: f64,
                      views: &Views)
                      -> (Pose, bool, UpdateResults) {
        let &(pose, ref shape) = match views.get(&id) {
            Some(tuple) => tuple,
            None => {
                return (self.pose, true, UpdateResults::empty());
            }
        };

        if self.can_shoot(&pose, shape) {
            (self.pose, true, UpdateResults::from_command(id, state.clone()))
        } else {
            let new_pose = self.pose
                .rotate_towards(pose.x, pose.y, 1.2 * dt)
                .move_towards(pose.x, pose.y, self.speed * dt);
            (new_pose, false, UpdateResults::empty())
        }
    }

    fn update_look(&self, x: f64, y: f64, dt: f64) -> (Pose, bool) {
        let new_pose = self.pose
            .rotate_towards(x, y, 1.2 * dt)
            .move_towards(x, y, self.speed * dt);
        (new_pose, self.can_see_point(x, y))
    }

    #[allow(float_cmp)]
    fn update_move(&self, x: f64, y: f64, dt: f64) -> (Pose, bool) {
        let new_pose = self.pose.move_towards(x, y, self.speed * dt);
        (new_pose, self.pose.x == x && self.pose.y == y)
    }

    fn update_shoot(&self, id: Id, dt: f64, views: &Views) -> (Pose, bool, UpdateResults) {
        let &(pose, ref shape) = match views.get(&id) {
            Some(tuple) => tuple,
            None => {
                return (self.pose, true, UpdateResults::empty());
            }
        };

        if self.can_shoot(&pose, shape) {
            let bullet_pose = self.pose.move_towards(pose.x, pose.y, self.width);
            let bullet = Unit::new(UnitRole::Bullet,
                                   Id::new_v4(),
                                   bullet_pose.x,
                                   bullet_pose.y,
                                   bullet_pose.rotation,
                                   self.team,
                                   UnitState::Move(pose.x, pose.y));
            (self.pose, true, UpdateResults::from_unit(bullet))
        } else {
            let new_pose = self.pose
                .rotate_towards(pose.x, pose.y, 1.2 * dt)
                .move_towards(pose.x, pose.y, self.speed * dt);
            (new_pose, false, UpdateResults::empty())
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

    fn can_see_point(&self, x: f64, y: f64) -> bool {
        self.fov().contains_point(&self.pose.isometry(), &Point2::new(x, y))
    }

    fn fov(&self) -> ConvexHull<Point2<f64>> {
        ConvexHull::new(FOV_POINTS.iter().map(|p| Point2::new(p[0], p[1])).collect())
    }

    fn range(&self) -> ConvexHull<Point2<f64>> {
        ConvexHull::new(RANGE_POINTS.iter().map(|p| Point2::new(p[0], p[1])).collect())
    }

    fn next_state(&mut self) -> UnitState {
        self.state_queue.pop().unwrap_or(UnitState::Idle)
    }

    fn peek_next_state(&self) -> &UnitState {
        if self.state_queue.is_empty() {
            IDLE
        } else {
            &self.state_queue[0]
        }
    }
}
