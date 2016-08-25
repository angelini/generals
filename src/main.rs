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
use std::f64;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use interpreter::{Delta, Error, Interpreter};
use unit::{EventType, GREEN, Id, Ids, Unit, UnitState};

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

        try!(self.run_all_unit_updates(args));
        try!(self.run_all_collisions());
        try!(self.run_all_views());

        loop {
            match self.delta_rx.try_recv() {
                Ok(delta) => self.apply_delta(delta),
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

        let run_time = (time::precise_time_ns() - time_start) as f64 / BILLION as f64;
        if run_time > 0.001 {
            info!(target: "timing", "... {:.*}", 5, run_time);
        } else {
            info!(target: "timing", ".");
        }

        Ok(())
    }

    fn run_all_unit_updates(&mut self, args: &UpdateArgs) -> Result<(), Error> {
        let mut changed = HashSet::new();
        let mut new_units = vec![];

        let views = self.units
            .values()
            .map(|u| {
                let map = self.units
                    .keys()
                    .map(|id| (*id, self.units.get(id).unwrap().xy()))
                    .collect::<HashMap<Id, (f64, f64)>>();
                (u.id, map)
            })
            .collect::<HashMap<Id, HashMap<Id, (f64, f64)>>>();

        for unit in self.units.values_mut() {
            let original_state = unit.state;
            let view = views.get(&unit.id);
            let mut new_units_chunk = unit.update(args, view.unwrap());

            new_units.append(&mut new_units_chunk);

            if unit.state != original_state {
                changed.insert(unit.id);
            }
        }

        for unit in new_units.into_iter() {
            self.add_unit(unit)
        }

        for unit in self.units.values() {
            if changed.contains(&unit.id) {
                if let Some(ref script) = unit.get_handler(&EventType::StateChange) {
                    try!(self.interpreter.exec(script, unit, None))
                }
            }
        }

        Ok(())
    }

    fn run_all_collisions(&mut self) -> Result<(), Error> {
        let units = &self.units;

        for id in units.keys() {
            let unit = self.units.get(id).unwrap();
            let seen = self.collision_cache.remove(id).unwrap();
            let current_view = try!(Self::run_collisions(&mut self.interpreter, unit, &seen, units));
            self.collision_cache.insert(*id, current_view);
        }

        Ok(())
    }

    fn run_collisions(interp: &mut Interpreter, unit: &Unit, collisions: &Ids, units: &HashMap<Id, Unit>) -> Result<Ids, Error> {
        let script = unit.get_handler(&EventType::Collision);

        if !script.is_some() {
            return Ok(HashSet::new());
        }

        let current_collisions = Self::detect_collisions(units, unit);

        if let Some(ref script) = script {
            for collision_id in &current_collisions {
                if !collisions.contains(collision_id) {
                    let collision = units.get(collision_id).unwrap();
                    try!(interp.exec(script, unit, Some(collision)))
                }
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

    fn run_views(interp: &mut Interpreter, unit: &Unit, seen: &Ids, units: &HashMap<Id, Unit>) -> Result<Ids, Error> {
        let enter_script = unit.get_handler(&EventType::EnterView);
        let exit_script = unit.get_handler(&EventType::ExitView);

        if !(enter_script.is_some() || exit_script.is_some()) {
            return Ok(HashSet::new());
        }

        let current_views = Self::detect_views(units, unit);

        if let Some(ref script) = enter_script {
            for view_id in &current_views {
                if !seen.contains(view_id) {
                    let other = units.get(view_id).unwrap();
                    try!(interp.exec(script, unit, Some(other)))
                }
            }
        }

        let not_seen = seen.difference(&current_views).cloned().collect::<Ids>();

        if let Some(ref script) = exit_script {
            for view_id in not_seen {
                let other = units.get(&view_id);
                try!(interp.exec(script, unit, other))
            }
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

    fn apply_delta(&mut self, delta: Delta) {
        let mut unit = self.units.get_mut(&delta.id).unwrap();
        if unit.state != UnitState::Dead {
            info!(target: "deltas",
                  "- {:?} {:?} -> {:?}", unit.role, unit.state, delta.state);
            unit.state = delta.state;
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
        Unit::new_general(25.0, 25.0, 1, UnitState::Move(375.0, 375.0)),

        Unit::new_soldier(50.0, 350.0, 1, UnitState::Look(50.0, 300.0)),
        Unit::new_soldier(150.0, 350.0, 1, UnitState::Look(150.0, 300.0)),
        Unit::new_soldier(250.0, 350.0, 1, UnitState::Look(250.0, 300.0)),
        Unit::new_soldier(350.0, 350.0, 1, UnitState::Look(350.0, 300.0)),

        Unit::new_soldier(100.0, 50.0, 2, UnitState::Move(200.0, 400.0)),
        Unit::new_soldier(175.0, 70.0, 2, UnitState::Move(300.0, 400.0)),
        Unit::new_soldier(90.0, 45.0, 2, UnitState::Move(120.0, 400.0)),
        Unit::new_soldier(300.0, 30.0, 2, UnitState::Move(100.0, 400.0)),
    ];

    units[0].rotation = 2.0;
    units[5].rotation = f64::consts::PI;
    units[6].rotation = f64::consts::PI;
    units[7].rotation = f64::consts::PI;
    units[8].rotation = f64::consts::PI;

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
                match state.update(&args) {
                    Ok(_) => {},
                    Err(err) => panic!(err)
                }
            }
            _ => {}
        }
    }
}
