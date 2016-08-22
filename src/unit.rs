use nalgebra::{Isometry2, Point2, Vector1, Vector2};
use ncollide::query::{self, PointQuery, Proximity};
use ncollide::shape::{ConvexHull, Cuboid};
use piston_window::*;
use regex::Regex;
use std::collections::HashSet;
use std::f64;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::str::FromStr;
use std::path::Path;
use uuid::Uuid;

pub type Color = [f32; 4];
pub type Id = Uuid;
pub type Ids = HashSet<Id>;

pub const BLUE: Color = [0.0, 0.0, 1.0, 1.0];
pub const PURPLE: Color = [0.5, 0.5, 1.0, 1.0];
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
    Move(f64, f64),
    Aim(f64, f64),
    Shoot(f64, f64),
    Dead,
}

impl ToString for UnitState {
    fn to_string(&self) -> String {
        match *self {
            UnitState::Move(x, y) => format!("move({:.*}, {:.*})", 2, x, 2, y),
            UnitState::Aim(x, y) => format!("aim({:.*}, {:.*})", 2, x, 2, y),
            UnitState::Shoot(x, y) => format!("shoot({:.*}, {:.*})", 2, x, 2, y),
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

        let re = Regex::new(r"move\((?P<x>\d+.\d+), (?P<y>\d+.\d+)\)").unwrap();
        if let Some(caps) = re.captures(s) {
            let x = f64::from_str(caps.name("x").unwrap()).unwrap();
            let y = f64::from_str(caps.name("y").unwrap()).unwrap();
            return Ok(UnitState::Move(x, y));
        };

        let re = Regex::new(r"aim\((?P<x>\d+.\d+), (?P<y>\d+.\d+)\)").unwrap();
        if let Some(caps) = re.captures(s) {
            let x = f64::from_str(caps.name("x").unwrap()).unwrap();
            let y = f64::from_str(caps.name("y").unwrap()).unwrap();
            return Ok(UnitState::Aim(x, y));
        };

        let re = Regex::new(r"shoot\((?P<x>\d+.\d+), (?P<y>\d+.\d+)\)").unwrap();
        if let Some(caps) = re.captures(s) {
            let x = f64::from_str(caps.name("x").unwrap()).unwrap();
            let y = f64::from_str(caps.name("y").unwrap()).unwrap();
            return Ok(UnitState::Shoot(x, y));
        };

        Err(s.to_string())
    }
}

#[derive(Debug)]
pub enum EventType {
    Collision,
    StateChange,
    EnterView,
    ExitView,
}

#[derive(Clone, Debug, PartialEq)]
struct EventHandlers {
    on_collision: Option<String>,
    on_state_change: Option<String>,
    on_enter_view: Option<String>,
    on_exit_view: Option<String>,
}

impl EventHandlers {
    fn new() -> EventHandlers {
        EventHandlers {
            on_collision: None,
            on_state_change: None,
            on_enter_view: None,
            on_exit_view: None,
        }
    }

    fn load(&mut self, prefix: &str) {
        if let Ok(script) = Self::read_script(prefix, "on_collision") {
            self.on_collision = Some(script);
        }
        if let Ok(script) = Self::read_script(prefix, "on_state_change") {
            self.on_state_change = Some(script);
        }
        if let Ok(script) = Self::read_script(prefix, "on_enter_view") {
            self.on_enter_view = Some(script);
        }
        if let Ok(script) = Self::read_script(prefix, "on_exit_view") {
            self.on_exit_view = Some(script);
        }
    }

    fn get(&self, event_type: &EventType) -> Option<&str> {
        let script = match *event_type {
            EventType::Collision => self.on_collision.as_ref(),
            EventType::StateChange => self.on_state_change.as_ref(),
            EventType::EnterView => self.on_enter_view.as_ref(),
            EventType::ExitView => self.on_exit_view.as_ref(),
        };
        match script {
            Some(s) => Some(s.as_str()),
            None => None,
        }
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
    event_handlers: EventHandlers,
}

impl Unit {
    fn new(role: UnitRole,
           x: f64,
           y: f64,
           team: usize,
           width: f64,
           speed: f64,
           state: UnitState)
           -> Unit {
        let mut handlers = EventHandlers::new();
        let color = match role {
            UnitRole::Soldier => {
                if team == 1 {
                    BLUE
                } else {
                    PURPLE
                }
            }
            UnitRole::General => RED,
            UnitRole::Bullet => BLACK,
        };

        handlers.load(&role.to_string());

        Unit {
            id: Uuid::new_v4(),
            team: team,
            color: color,
            x: x,
            y: y,
            width: width,
            speed: speed,
            rotation: 0.0,
            shape: Cuboid::new(Vector2::new(width * 0.5, width * 0.5)),
            role: role,
            state: state,
            state_queue: Vec::new(),
            event_handlers: handlers,
        }
    }

    pub fn new_general(x: f64, y: f64, team: usize, state: UnitState) -> Unit {
        Self::new(UnitRole::General, x, y, team, 50.0, 50.0, state)
    }

    pub fn new_soldier(x: f64, y: f64, team: usize, state: UnitState) -> Unit {
        Self::new(UnitRole::Soldier, x, y, team, 25.0, 150.0, state)
    }

    pub fn new_bullet(x: f64, y: f64, team: usize, state: UnitState) -> Unit {
        Self::new(UnitRole::Bullet, x, y, team, 5.0, 150.0, state)
    }

    pub fn update(&mut self, args: &UpdateArgs) -> Vec<Unit> {
        match self.state {
            UnitState::Move(x, y) => {
                let dist = self.speed * args.dt;

                if x < self.x + dist && x > self.x - dist && y < self.y + dist &&
                   y > self.y - dist {
                    self.x = x;
                    self.y = y;

                    let original_state = self.state.clone();
                    self.state = self.next_state();
                    info!(target: "unit-state",
                        "{:?} {:?} -> {:?}", self.role, original_state, self.state);

                    return vec![];
                }

                self.move_self_towards(x, y, args.dt);
                vec![]
            }
            UnitState::Aim(x, y) => {
                let dx = x - self.x;
                let dy = y - self.y;

                let mut dest_rotation = (dy.atan2(dx) + 0.5 * f64::consts::PI) %
                                        (2.0 * f64::consts::PI);
                let curr_rotation = self.rotation % (2.0 * f64::consts::PI);

                if dest_rotation < 0.0 {
                    dest_rotation += 2.0 * f64::consts::PI;
                }

                if dest_rotation < curr_rotation + args.dt &&
                   dest_rotation > curr_rotation - args.dt {
                    self.rotation = dest_rotation;

                    if self.can_see_point(x, y) {
                        let original_state = self.state.clone();
                        self.state = self.next_state();
                        info!(target: "unit-state",
                            "{:?} {:?} -> {:?}", self.role, original_state, self.state);
                    } else {
                        self.move_self_towards(x, y, args.dt);
                    }

                    return vec![];
                }

                let delta = dest_rotation - curr_rotation;
                if delta > 0.0 && delta < f64::consts::PI {
                    self.rotation += args.dt;
                } else {
                    self.rotation -= args.dt;
                    if self.rotation < 0.0 {
                        self.rotation += 2.0 * f64::consts::PI;
                    }
                }
                vec![]
            }
            UnitState::Shoot(x, y) => {
                if !self.can_see_point(x, y) {
                    self.state = UnitState::Aim(x, y);
                    self.state_queue.push(UnitState::Shoot(x, y));
                    return vec![];
                }
                self.state = self.next_state();

                let (xdelta, ydelta) = self.move_towards(x, y, self.width + 10.0);
                vec![Unit::new_bullet(self.x + xdelta,
                                      self.y + ydelta,
                                      self.team,
                                      UnitState::Move(x, y))]
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

    pub fn get_handler(&self, event_type: &EventType) -> Option<&str> {
        self.event_handlers.get(event_type)
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

    fn move_self_towards(&mut self, x: f64, y: f64, dt: f64) {
        let dist = self.speed * dt;
        let (xdelta, ydelta) = self.move_towards(x, y, dist);
        self.x += xdelta;
        self.y += ydelta;
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
}
