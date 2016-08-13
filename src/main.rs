#![feature(plugin)]

#![plugin(clippy)]

extern crate env_logger;
extern crate hlua;
#[macro_use]
extern crate log;
extern crate nalgebra;
extern crate ncollide;
extern crate piston_window;
extern crate regex;
extern crate time;
extern crate uuid;

mod interpreter;
mod unit;

use piston_window::*;
use std::collections::{HashMap, HashSet};

use interpreter::{Delta, Interpreter};
use unit::{EventType, GREEN, Id, Ids, Unit, UnitState};

const BILLION: u64 = 1000000000;

struct State<'a> {
    interpreter: Interpreter<'a>,
    units: HashMap<Id, Unit>,
    collision_cache: HashMap<Id, Ids>,
    view_cache: HashMap<Id, Ids>,
}

impl<'a> State<'a> {
    fn new() -> State<'a> {
        State {
            interpreter: Interpreter::new(),
            units: HashMap::new(),
            collision_cache: HashMap::new(),
            view_cache: HashMap::new(),
        }
    }

    fn add_unit(&mut self, unit: Unit) {
        self.collision_cache.insert(unit.id, HashSet::new());
        self.view_cache.insert(unit.id, HashSet::new());
        self.units.insert(unit.id, unit);
    }

    fn update(&mut self, args: &UpdateArgs) {
        let time_start = time::precise_time_ns();

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
            info!(target: "deltas", "views ({})", deltas.len());
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

        info!(target: "timing", "update {:.*}", 5,
              (time::precise_time_ns() - time_start) as f64 / BILLION as f64);
    }

    fn run_all_unit_updates(&mut self, args: &UpdateArgs) -> Vec<Delta> {
        let interpreter = &mut self.interpreter;
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
            .map(|(_, unit)| Self::run_unit_update(interpreter, unit))
            .filter(|u| u.is_some())
            .map(|u| u.unwrap())
            .collect::<Vec<Delta>>()
    }

    fn run_unit_update(interpreter: &mut Interpreter, unit: &Unit) -> Option<Delta> {
        match unit.get_handler(&EventType::StateChange) {
            Some(ref script) => Some(interpreter.exec(unit, script, None)),
            None => None,
        }
    }

    fn run_all_collisions(&mut self) -> Vec<Delta> {
        let interpreter = &mut self.interpreter;
        let units = &self.units;
        let collision_cache = &mut self.collision_cache;

        self.units
            .keys()
            .flat_map(|id| {
                let unit = units.get(id).unwrap();
                let script = unit.get_handler(&EventType::Collision);

                if !script.is_some() {
                    return vec![];
                }

                let mut collides = collision_cache.get_mut(id).unwrap();
                let (deltas, current_collides) =
                    Self::run_collisions(interpreter, units, unit, collides, script);

                collides.clear();
                for collide in current_collides {
                    collides.insert(collide);
                }

                deltas
            })
            .collect::<Vec<Delta>>()
    }

    fn run_collisions(interpreter: &mut Interpreter,
                      units: &HashMap<Id, Unit>,
                      unit: &Unit,
                      collides: &Ids,
                      script: Option<&str>)
                      -> (Vec<Delta>, Ids) {
        let current_collides = Self::detect_collisions(units, unit);

        match script {
            Some(ref script) => {
                let deltas = current_collides.iter()
                    .filter(|collide_id| !collides.contains(collide_id))
                    .map(|collide_id| {
                        let collide = units.get(collide_id).unwrap();
                        interpreter.exec(unit, script, Some(collide))
                    })
                    .collect::<Vec<Delta>>();
                (deltas, current_collides)
            }
            None => (vec![], current_collides),
        }
    }

    fn run_all_views(&mut self) -> Vec<Delta> {
        let interpreter = &mut self.interpreter;
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
                    Self::run_enter_views(interpreter, units, unit, seen, enter_script);

                let not_seen = seen.difference(&current_view).cloned().collect::<Ids>();
                let mut exit_deltas =
                    Self::run_exit_views(interpreter, units, unit, &not_seen, exit_script);

                seen.clear();
                for view in current_view {
                    seen.insert(view);
                }

                enter_deltas.append(&mut exit_deltas);
                enter_deltas
            })
            .collect::<Vec<Delta>>()
    }

    fn run_enter_views(interpreter: &mut Interpreter,
                       units: &HashMap<Id, Unit>,
                       unit: &Unit,
                       seen: &Ids,
                       script: Option<&str>)
                       -> (Vec<Delta>, Ids) {
        let current_views = Self::detect_views(units, unit);

        match script {
            Some(ref script) => {
                let deltas = current_views.iter()
                    .filter(|view_id| !seen.contains(view_id))
                    .map(|view_id| {
                        let other = units.get(view_id).unwrap();
                        interpreter.exec(unit, script, Some(other))
                    })
                    .collect::<Vec<Delta>>();
                (deltas, current_views)
            }
            None => (vec![], current_views),
        }
    }

    fn run_exit_views(interpreter: &mut Interpreter,
                      units: &HashMap<Id, Unit>,
                      unit: &Unit,
                      not_seen: &Ids,
                      script: Option<&str>)
                      -> Vec<Delta> {
        // FIXME: does not fire event when unit dies
        match script {
            Some(ref script) => {
                not_seen.iter()
                    .map(|other_id| units.get(other_id))
                    .filter(|other| other.is_some())
                    .map(|other| interpreter.exec(unit, script, other))
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

    fn detect_views(units: &HashMap<Id, Unit>, unit: &Unit) -> Ids {
        units.iter()
            .filter(|&(id, _)| &unit.id != id)
            .filter(|&(_, u)| unit.can_see(u))
            .map(|(view_id, _)| *view_id)
            .collect()
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

    let mut units = vec![
        Unit::new_general(25.0, 25.0),
        Unit::new_soldier(200.0, 300.0),
        Unit::new_soldier(350.0, 350.0),
        Unit::new_bullet(400.0, 400.0),
    ];

    units[0].rotation = 0.0;

    units[0].state = UnitState::Shooting(300.0, -100.0);
    units[1].state = UnitState::Moving(0.0, 375.0);
    units[2].state = UnitState::Moving(300.0, 200.0);
    units[3].state = UnitState::Moving(300.0, 0.0);

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
