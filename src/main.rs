#![feature(plugin)]

#![plugin(clippy)]

extern crate hlua;
extern crate piston_window;
extern crate regex;

mod unit;

use hlua::Lua;
use piston_window::*;
use std::str::FromStr;

use unit::{GREEN, Unit, UnitState};

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
        self.update_units(args);
        self.detect_and_run_collisions();

        self.units.retain(|u| {
            match u.state {
                UnitState::Dead => false,
                _ => true,
            }
        });
    }

    fn update_units(&mut self, args: &UpdateArgs) {
        let mut lua = &mut self.lua;

        for unit in &mut self.units {
            let original_state = unit.state.clone();
            unit.update(args);

            if original_state != unit.state && unit.on_state_change.is_some() {
                let script = unit.on_state_change.clone().unwrap();
                Self::exec_lua(&mut lua, unit, &script, None);
            }
        }
    }

    fn detect_and_run_collisions(&mut self) {
        let units = self.units.clone();
        let mut lua = &mut self.lua;

        for unit in &mut self.units {
            let collisions = units.iter()
                .filter(|u| *u != unit)
                .filter(|u| unit.overlaps(u))
                .collect::<Vec<&Unit>>();

            if let Some(script) = unit.on_collision.clone() {
                for collision in collisions {
                    Self::exec_lua(&mut lua, unit, &script, Some(collision));
                }
            }
        }
    }

    fn exec_lua(lua: &mut Lua, unit: &mut Unit, script: &str, other: Option<&Unit>) {
        lua.set("x", unit.x);
        lua.set("y", unit.y);
        lua.set("role", unit.role.to_string());
        lua.set("state", unit.state.to_string());

        if let Some(other_unit) = other {
            lua.set("other_role", other_unit.role.to_string());
            lua.set("other_state", other_unit.state.to_string());
        }

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

    let mut units = vec![Unit::new_general(50.0, 50.0),
                         Unit::new_soldier(200.0, 300.0),
                         Unit::new_soldier(350.0, 350.0),
                         Unit::new_bullet(290.0, 290.0)];

    units[0].rotation = 1.0;

    units[0].state = UnitState::Moving(100.0, 100.0);
    units[1].state = UnitState::Moving(0.0, 375.0);
    units[2].state = UnitState::Moving(300.0, 200.0);
    units[3].state = UnitState::Moving(0.0, 0.0);

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
