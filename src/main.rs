#![feature(plugin)]

#![plugin(clippy)]

extern crate env_logger;
extern crate hlua;
#[macro_use] extern crate log;
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

use unit::{EventType, GREEN, Id, Ids, Unit, UnitState};

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
        if !deltas.is_empty() {
            info!(target: "deltas", "units ({})", deltas.len());
        }
        self.apply_deltas(deltas);

        let deltas = self.run_all_collisions();
        if !deltas.is_empty() {
            info!(target: "deltas", "collisions ({})", deltas.len());
        }
        self.apply_deltas(deltas);

        let deltas = self.run_all_views();
        if !deltas.is_empty() {
            info!(target: "deltas", "views ({}):", deltas.len());
        }
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

        match unit.get_handler(&EventType::StateChange) {
            Some(ref script) => Some(Self::exec_lua(&mut lua, unit, script, None)),
            None => None,
        }
    }

    fn apply_deltas(&mut self, deltas: Vec<Delta>) {
        for delta in deltas {
            let mut unit = self.units.get_mut(&delta.id).unwrap();

            if unit.state != UnitState::Dead {
                info!(target: "deltas", "- {:?} {:?} -> {:?}", unit.role, unit.state, delta.state);
                unit.state = delta.state;
            }
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

        match unit.get_handler(&EventType::Collision) {
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

    fn run_all_views(&mut self) -> Vec<Delta> {
        let lua = &mut self.lua;
        let units = &self.units;
        let view_cache = &mut self.view_cache;

        self.units
            .keys()
            .flat_map(|id| {
                let unit = units.get(id).unwrap();
                let enter_script = unit.get_handler(&EventType::EnterView);
                let exit_script = unit.get_handler(&EventType::ExitView);

                if !(enter_script.is_some() || exit_script.is_some()) {
                    return vec![];
                }

                let mut seen = view_cache.get_mut(id).unwrap();
                let (mut enter_deltas, current_view) =
                    Self::run_enter_views(lua, units, unit, seen, enter_script);

                let not_seen = seen.difference(&current_view).cloned().collect::<Ids>();
                let mut exit_deltas =
                    Self::run_exit_views(lua, units, unit, &not_seen, exit_script);

                seen.clear();
                for view in current_view {
                    seen.insert(view);
                }

                enter_deltas.append(&mut exit_deltas);
                enter_deltas
            })
            .collect::<Vec<Delta>>()
    }

    fn run_enter_views(lua: &mut Lua,
                       units: &HashMap<Id, Unit>,
                       unit: &Unit,
                       seen: &Ids,
                       script: Option<&str>)
                       -> (Vec<Delta>, Ids) {
        let mut lua = lua;
        let current_view = Self::detect_views(units, unit);

        match script {
            Some(ref script) => {
                let deltas = current_view.iter()
                    .filter(|view_id| !seen.contains(view_id))
                    .map(|other_id| {
                        let other = units.get(other_id).unwrap();
                        Self::exec_lua(&mut lua, unit, script, Some(other))
                    })
                    .collect::<Vec<Delta>>();
                (deltas, current_view)
            }
            None => (vec![], current_view),
        }
    }

    fn run_exit_views(lua: &mut Lua,
                      units: &HashMap<Id, Unit>,
                      unit: &Unit,
                      not_seen: &Ids,
                      script: Option<&str>)
                      -> Vec<Delta> {
        let mut lua = lua;

        // FIXME: does not fire event when unit dies
        match script {
            Some(ref script) => {
                not_seen.iter()
                    .map(|other_id| units.get(other_id))
                    .filter(|other| other.is_some())
                    .map(|other| {
                        Self::exec_lua(&mut lua, unit, script, other)
                    })
                    .collect::<Vec<Delta>>()
            }
            None => vec![],
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
    env_logger::init().unwrap();
    let mut window: PistonWindow = WindowSettings::new("example", [400, 400])
        .exit_on_esc(true)
        .build()
        .unwrap();

    let mut units = vec![Unit::new_general(25.0, 25.0),
                         Unit::new_soldier(200.0, 300.0),
                         Unit::new_soldier(350.0, 350.0),
                         Unit::new_bullet(400.0, 400.0)];

    units[0].rotation = 1.0;

    units[0].state = UnitState::Moving(100.0, 100.0);
    units[1].state = UnitState::Moving(0.0, 375.0);
    units[2].state = UnitState::Moving(300.0, 200.0);
    units[3].state = UnitState::Moving(0.0, 0.0);

    let mut state = State::new();
    for unit in units {
        info!(target: "units", "{} {:?}", unit.id, unit.role);
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
