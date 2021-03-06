#![feature(plugin)]

#![plugin(clippy)]

extern crate env_logger;
extern crate hlua;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate nalgebra;
extern crate ncollide;
extern crate piston_window;
extern crate regex;
extern crate time;
extern crate uuid;

mod geometry;
mod interpreter;
mod parser;
mod unit;

use piston_window::*;
use std::collections::{HashMap, HashSet};
use std::f64;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use interpreter::{Delta, Error, EventType, Interpreter};
use unit::{GREEN, Id, Ids, Unit, UnitState, Views};

const BILLION: u64 = 1000000000;

struct State {
    interpreter: Interpreter,
    units: HashMap<Id, Unit>,
    collision_cache: HashMap<Id, Ids>,
    view_cache: HashMap<Id, Ids>,
    delta_rx: Receiver<Delta>,
}

impl State {
    fn new() -> State {
        let (tx, rx) = mpsc::channel();
        State {
            interpreter: Interpreter::new(tx),
            units: HashMap::new(),
            collision_cache: HashMap::new(),
            view_cache: HashMap::new(),
            delta_rx: rx,
        }
    }

    fn add_unit(&mut self, unit: Unit) {
        self.collision_cache.insert(unit.id, HashSet::new());
        self.view_cache.insert(unit.id, HashSet::new());
        self.units.insert(unit.id, unit);
    }

    fn update(&mut self, args: &UpdateArgs) -> Result<(), Error> {
        let time_start = time::precise_time_ns();
        let mut changed = vec![];

        try!(self.run_all_unit_updates(args));
        try!(self.run_all_collisions());
        try!(self.run_all_views());

        loop {
            match self.delta_rx.try_recv() {
                Ok(delta) => {
                    if let Some(id) = self.apply_delta(delta) {
                        changed.push(id)
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => panic!("delta_rx disconnected"),
            }
        }

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

        for id in changed {
            if let Some(unit) = self.units.get(&id) {
                try!(self.interpreter.exec(&unit.role, &EventType::StateChange, unit, None))
            }
        }

        let run_time = (time::precise_time_ns() - time_start) as f64 / BILLION as f64;
        if run_time > 0.001 {
            info!(target: "timing", "... {:.*}", 5, run_time);
        } else {
            info!(target: "timing", ".");
        }

        if time_start % 1000 == 0 {
            info!(target: "units", "---");
            for unit in self.units.values() {
                info!(target: "units", "{} {:?} {:?}", unit.id, unit.role, unit.state);
            }
            info!(target: "units", "---");
        }

        Ok(())
    }

    fn run_all_unit_updates(&mut self, args: &UpdateArgs) -> Result<(), Error> {
        let mut changed = HashSet::new();
        let mut commands = HashMap::new();
        let mut new_units = vec![];

        let views = self.units
            .values()
            .map(|u| {
                let map = self.units
                    .keys()
                    .map(|id| {
                        let unit = self.units.get(id).unwrap();
                        (*id, (unit.pose, unit.shape.clone()))
                    })
                    .collect::<Views>();
                (u.id, map)
            })
            .collect::<HashMap<Id, Views>>();

        for unit in self.units.values_mut() {
            let original_state = unit.state.clone();
            let view = views.get(&unit.id).unwrap();

            let update_results = unit.update(args, view);

            if let Some((id, state)) = update_results.command {
                commands.insert(id, state);
            }
            if let Some(unit) = update_results.unit {
                new_units.push(unit)
            }

            if unit.state != original_state {
                changed.insert(unit.id);
            }
        }

        for unit in self.units.values_mut() {
            if let Some(state) = commands.remove(&unit.id) {
                unit.state = state;
                changed.insert(unit.id);
            }
        }

        for unit in new_units.into_iter() {
            self.add_unit(unit)
        }

        for unit in self.units.values() {
            if changed.contains(&unit.id) {
                try!(self.interpreter.exec(&unit.role, &EventType::StateChange, unit, None));
            }
        }

        Ok(())
    }

    fn run_all_collisions(&mut self) -> Result<(), Error> {
        let units = &self.units;

        for id in units.keys() {
            let unit = self.units.get(id).unwrap();
            let seen = self.collision_cache.remove(id).unwrap();
            let current_view =
                try!(Self::run_collisions(&mut self.interpreter, unit, &seen, units));
            self.collision_cache.insert(*id, current_view);
        }

        Ok(())
    }

    fn run_collisions(interp: &mut Interpreter,
                      unit: &Unit,
                      collisions: &Ids,
                      units: &HashMap<Id, Unit>)
                      -> Result<Ids, Error> {
        let current_collisions = Self::detect_collisions(units, unit);

        for collision_id in &current_collisions {
            if !collisions.contains(collision_id) {
                let collision = units.get(collision_id).unwrap();
                try!(interp.exec(&unit.role, &EventType::Collision, unit, Some(collision)))
            }
        }
        Ok(current_collisions)
    }

    fn run_all_views(&mut self) -> Result<(), Error> {
        let units = &self.units;

        for id in units.keys() {
            let unit = self.units.get(id).unwrap();
            let seen = self.view_cache.remove(id).unwrap();
            let current_view = try!(Self::run_views(&mut self.interpreter, unit, &seen, units));
            self.view_cache.insert(*id, current_view);
        }

        Ok(())
    }

    fn run_views(interp: &mut Interpreter,
                 unit: &Unit,
                 seen: &Ids,
                 units: &HashMap<Id, Unit>)
                 -> Result<Ids, Error> {
        let current_views = Self::detect_views(units, unit);

        for view_id in &current_views {
            if !seen.contains(view_id) {
                let other = units.get(view_id).unwrap();
                try!(interp.exec(&unit.role, &EventType::EnterView, unit, Some(other)))
            }
        }

        let not_seen = seen.difference(&current_views).cloned().collect::<Ids>();

        for view_id in not_seen {
            let other = units.get(&view_id);
            try!(interp.exec(&unit.role, &EventType::ExitView, unit, other))
        }

        Ok(current_views)
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

    fn apply_delta(&mut self, delta: Delta) -> Option<Id> {
        match delta {
            Delta::UpdateState(id, state) => {
                match self.units.get_mut(&id) {
                    Some(unit) => {
                        if unit.state != UnitState::Dead && unit.state != state {
                            info!(target: "deltas",
                                  "- {:?} {:?} -> {:?}", unit.role, unit.state, state);
                            unit.state = state;
                            Some(id)
                        } else {
                            None
                        }
                    }
                    None => {
                        info!(target: "deltas",
                              "missing unit {}", id);
                        None
                    }
                }
            }
            Delta::NewUnit(role, id, x, y, rotation, team) => {
                self.add_unit(Unit::new(role, id, x, y, rotation, team, UnitState::Idle));
                None
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
    let mut window: PistonWindow = WindowSettings::new("example", geometry::SCENE_SIZE)
        .exit_on_esc(true)
        .build()
        .unwrap();

    let mut state = State::new();

    while let Some(e) = window.next() {
        match e {
            Event::Render(args) => {
                draw_units(&mut window, e, &args, &state);
            }
            Event::Update(args) => {
                match state.update(&args) {
                    Ok(_) => {}
                    Err(err) => panic!(err),
                }
            }
            _ => {}
        }
    }
}
