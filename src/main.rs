#![feature(plugin)]

#![plugin(clippy)]

extern crate hlua;
extern crate piston_window;
extern crate regex;

use hlua::Lua;
use piston_window::*;
use std::str::FromStr;
use regex::Regex;

type Color = [f32; 4];

const BLUE: Color = [0.0, 0.0, 1.0, 1.0];
const GREEN: Color = [0.0, 1.0, 0.0, 1.0];
const RED: Color = [1.0, 0.0, 0.0, 1.0];
const BLACK: Color = [0.0, 0.0, 0.0, 1.0];

#[derive(Clone, Debug, PartialEq)]
enum UnitState {
    Idle,
    Moving(f64, f64),
    Dead,
}

impl ToString for UnitState {
    fn to_string(&self) -> String {
        match *self {
            UnitState::Moving(x, y) => format!("moving({}, {})", x, y),
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

        Err(s.to_string())
    }
}

#[derive(Clone, Debug)]
struct Unit {
    color: Color,
    x: f64,
    y: f64,
    width: f64,
    speed: f64,
    state: UnitState,
    on_state_change: Option<String>,
}

impl Unit {
    fn new(color: Color, x: f64, y: f64, width: f64, speed: f64) -> Unit {
        Unit {
            color: color,
            x: x,
            y: y,
            width: width,
            speed: speed,
            state: UnitState::Idle,
            on_state_change: None,
        }
    }

    fn update(&mut self, args: &UpdateArgs) {
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
            UnitState::Idle | _ => {}
        }
    }

    fn render<G: Graphics>(&self, _: &RenderArgs, c: &Context, g: &mut G) {
        let square = rectangle::square(self.x, self.y, self.width);
        rectangle(self.color, square, c.transform, g);
    }
}

struct State<'a> {
    lua: Lua<'a>,
    units: Vec<Unit>,
}

impl<'a> State<'a> {
    fn new(units: Vec<Unit>) -> State<'a> {
        let mut lua = Lua::new();
        lua.openlibs();
        State {
            lua: lua,
            units: units,
        }
    }

    fn update(&mut self, args: &UpdateArgs) {
        let mut lua = &mut self.lua;

        for unit in &mut self.units {
            let original_state = unit.state.clone();
            unit.update(args);

            if original_state != unit.state && unit.on_state_change.is_some() {
                let script = unit.on_state_change.clone().unwrap();
                Self::exec_lua(&mut lua, unit, &script);
            }
        }

        self.units.retain(|u| {
            match u.state {
                UnitState::Dead => false,
                _ => true,
            }
        });
    }

    fn exec_lua(lua: &mut Lua, unit: &mut Unit, script: &str) {
        lua.set("x", unit.x);
        lua.set("y", unit.y);
        lua.set("state", unit.state.to_string());
        lua.execute::<()>(script).unwrap();

        let new_state: String = lua.get("state").unwrap();
        unit.state = match UnitState::from_str(&new_state) {
            Ok(state) => state,
            Err(_) => panic!("Invalid state: {}", new_state),
        };
        println!("lua.get(state): {:?}", new_state);
    }
}

fn draw_units(window: &mut PistonWindow, event: Event, args: &RenderArgs, state: &State) {
    window.draw_2d(&event, |c, g| {
        clear(GREEN, g);
        for unit in &state.units {
            unit.render(args, &c, g)
        }
    });
}

fn main() {
    let mut window: PistonWindow = WindowSettings::new("example", [400, 400])
        .exit_on_esc(true)
        .build()
        .unwrap();

    let mut units = vec![Unit::new(RED, 0.0, 0.0, 50.0, 50.0),
                         Unit::new(BLUE, 300.0, 300.0, 25.0, 50.0),
                         Unit::new(BLUE, 350.0, 350.0, 25.0, 50.0),
                         Unit::new(BLACK, 350.0, 350.0, 5.0, 200.0)];

    units[0].state = UnitState::Moving(100.0, 100.0);
    units[1].state = UnitState::Moving(0.0, 375.0);
    units[2].state = UnitState::Moving(300.0, 200.0);
    units[3].state = UnitState::Moving(100.0, 100.0);

    let move_back_on_idle = "
if state == \"idle\" and x ~= 0.0 then
  state = \"moving(0.0, 0.0)\"
end
";
    units[0].on_state_change = Some(move_back_on_idle.to_string());

    let move_random_on_idle = "
if state == \"idle\" then
  state = string.format(\"moving(%f, \
                               %f)\", math.random(350), math.random(350))
end
";
    units[1].on_state_change = Some(move_random_on_idle.to_string());
    units[2].on_state_change = Some(move_random_on_idle.to_string());

    let kill_on_idle = "
if state == \"idle\" then
  state = \"dead\"
end
";
    units[3].on_state_change = Some(kill_on_idle.to_string());

    let mut state = State::new(units);

    while let Some(e) = window.next() {
        match e {
            Event::Render(args) => {
                draw_units(&mut window, e, &args, &state);
            }
            Event::Update(args) => {
                state.update(&args);
            }
            _ => {}
        }
    }
}
