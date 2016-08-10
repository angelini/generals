#![feature(plugin)]

#![plugin(clippy)]

extern crate hlua;
extern crate nalgebra;
extern crate ncollide;
extern crate piston_window;
extern crate regex;
extern crate uuid;

mod unit;

use hlua::Lua;
use piston_window::*;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use unit::{GREEN, Id, Ids, Unit, UnitState};

#[derive(Debug)]
struct Delta {
    id: Id,
    state: UnitState,
}

struct State<'a> {
    lua: Lua<'a>,
    units: HashMap<Id, Unit>,
    view_cache: HashMap<Id, Ids>,
}

impl<'a> State<'a> {
    fn new() -> State<'a> {
        let mut lua = Lua::new();
        lua.openlibs();
        State {
            lua: lua,
            units: HashMap::new(),
            view_cache: HashMap::new(),
        }
    }

    fn add_unit(&mut self, unit: Unit) {
        self.view_cache.insert(unit.id, HashSet::new());
        self.units.insert(unit.id, unit);
    }

    fn update(&mut self, args: &UpdateArgs) {
        let deltas = self.run_all_unit_updates(args);
        // println!(">> unit update deltas: {:?}", deltas.len());
        self.apply_deltas(deltas);

        let deltas = self.run_all_collisions();
        // println!(">> collision deltas: {:?}", deltas.len());
        self.apply_deltas(deltas);

        let deltas = self.run_all_enter_views();
        // println!(">> view deltas: {:?}", deltas.len());
        self.apply_deltas(deltas);

        let dead_units = self.units
            .iter()
            .filter(|&(_, u)| {
                match u.state {
                    UnitState::Dead => true,
                    _ => false,
                }
            })
            .map(|(k, _)| *k)
            .collect::<Ids>();

        for dead_unit in dead_units {
            self.units.remove(&dead_unit);
        }
    }

    fn run_all_unit_updates(&mut self, args: &UpdateArgs) -> Vec<Delta> {
        let lua = &mut self.lua;
        let mut changed = HashSet::new();

        for unit in self.units.values_mut() {
            let original_state = unit.state.clone();
            unit.update(args);

            if unit.state != original_state {
                changed.insert(unit.id);
            }
        }

        self.units
            .iter()
            .filter(|&(id, _)| changed.contains(id))
            .map(|(_, unit)| Self::run_unit_update(lua, unit))
            .filter(|u| u.is_some())
            .map(|u| u.unwrap())
            .collect::<Vec<Delta>>()
    }

    fn run_unit_update(lua: &mut Lua, unit: &Unit) -> Option<Delta> {
        let mut lua = lua;

        match unit.on_state_change {
            Some(ref script) => Some(Self::exec_lua(&mut lua, unit, script, None)),
            None => None,
        }
    }

    fn apply_deltas(&mut self, deltas: Vec<Delta>) {
        for delta in deltas {
            let mut unit = self.units.get_mut(&delta.id).unwrap();
            println!("applying: {:?}", delta);
            println!("to: {:?}", unit);
            unit.state = delta.state;
        }
    }

    fn run_all_collisions(&mut self) -> Vec<Delta> {
        let lua = &mut self.lua;
        let units = &self.units;
        self.units
            .keys()
            .flat_map(|key| Self::run_collisions(lua, units, key))
            .collect::<Vec<Delta>>()
    }

    fn run_collisions(lua: &mut Lua, units: &HashMap<Id, Unit>, id: &Id) -> Vec<Delta> {
        let mut lua = lua;
        let unit = units.get(id).unwrap();

        match unit.on_collision {
            Some(ref script) => {
                Self::detect_collisions(units, unit)
                    .into_iter()
                    .map(|collide_id| {
                        let collision = units.get(&collide_id).unwrap();
                        Self::exec_lua(&mut lua, unit, script, Some(collision))
                    })
                    .collect::<Vec<Delta>>()
            }
            None => vec![],
        }
    }

    fn detect_collisions(units: &HashMap<Id, Unit>, unit: &Unit) -> Ids {
        units.iter()
            .filter(|&(id, _)| &unit.id != id)
            .filter(|&(_, u)| unit.overlaps(u))
            .map(|(collide_id, _)| *collide_id)
            .collect()
    }

    fn run_all_enter_views(&mut self) -> Vec<Delta> {
        let lua = &mut self.lua;
        let units = &self.units;
        let view_cache = &mut self.view_cache;

        self.units
            .keys()
            .flat_map(|id| {
                let mut seen = view_cache.get_mut(id).unwrap();
                let (deltas, new_seen) = Self::run_enter_views(lua, units, id, seen);
                for new in new_seen {
                    seen.insert(new);
                }
                deltas
            })
            .collect::<Vec<Delta>>()
    }

    fn run_enter_views(lua: &mut Lua,
                       units: &HashMap<Id, Unit>,
                       id: &Id,
                       seen: &mut Ids)
                       -> (Vec<Delta>, Ids) {
        let mut lua = lua;
        let unit = units.get(id).unwrap();

        match unit.on_enter_view {
            Some(ref script) => {
                let mut new_seen = HashSet::new();
                let deltas = Self::detect_views(units, unit)
                    .into_iter()
                    .filter(|view_id| !seen.contains(view_id))
                    .map(|view_id| {
                        let view = units.get(&view_id).unwrap();
                        new_seen.insert(view_id);
                        Self::exec_lua(&mut lua, unit, script, Some(view))
                    })
                    .collect::<Vec<Delta>>();
                (deltas, new_seen)
            }
            None => (vec![], HashSet::new()),
        }
    }

    fn detect_views(units: &HashMap<Id, Unit>, unit: &Unit) -> Ids {
        units.iter()
            .filter(|&(id, _)| &unit.id != id)
            .filter(|&(_, u)| unit.can_see(u))
            .map(|(view_id, _)| *view_id)
            .collect()
    }

    fn exec_lua(lua: &mut Lua, unit: &Unit, script: &str, other: Option<&Unit>) -> Delta {
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
        println!("lua.get(state): {:?}", new_state);

        match UnitState::from_str(&new_state) {
            Ok(state) => {
                Delta {
                    id: unit.id,
                    state: state,
                }
            }
            Err(_) => panic!("Invalid state: {}", new_state),
        }
    }
}

fn draw_units(window: &mut PistonWindow, event: Event, args: &RenderArgs, state: &State) {
    window.draw_2d(&event, |c, g| {
        clear(GREEN, g);
        for unit in state.units.values() {
            unit.render(args, &c, g)
        }
    });
}

fn main() {
    let mut window: PistonWindow = WindowSettings::new("example", [400, 400])
        .exit_on_esc(true)
        .build()
        .unwrap();

    let mut units = vec![Unit::new_general(25.0, 25.0),
                         Unit::new_soldier(200.0, 300.0),
                         Unit::new_soldier(350.0, 350.0),
                         Unit::new_bullet(290.0, 290.0)];

    units[0].rotation = 1.0;

    units[0].state = UnitState::Moving(100.0, 100.0);
    units[1].state = UnitState::Moving(0.0, 375.0);
    units[2].state = UnitState::Moving(300.0, 200.0);
    units[3].state = UnitState::Moving(0.0, 0.0);

    let mut state = State::new();
    for unit in units {
        state.add_unit(unit)
    }

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
