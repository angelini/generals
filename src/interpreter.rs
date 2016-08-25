use hlua::{Lua, LuaTable};
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use unit::{Id, Unit, UnitRole, UnitState};

#[derive(Debug)]
pub struct Delta {
    pub id: Id,
    pub state: UnitState,
}

impl Delta {
    fn new(id: Id, state: UnitState) -> Delta {
        Delta {
            id: id,
            state: state,
        }
    }
}

struct UnitSnapshot {
    id: Id,
    x: f64,
    y: f64,
    team: usize,
    role: UnitRole,
    state: UnitState,
}

impl UnitSnapshot {
    fn new(unit: &Unit) -> UnitSnapshot {
        UnitSnapshot {
            id: unit.id,
            x: unit.x,
            y: unit.y,
            team: unit.team,
            role: unit.role,
            state: unit.state,
        }
    }
}

type ExecState = (String, UnitSnapshot, Option<UnitSnapshot>);

pub struct Interpreter {
    tx: Sender<ExecState>,
}

impl Interpreter {
    pub fn new(delta_tx: Sender<Delta>) -> Interpreter {
        let mut lua = Lua::new();
        lua.openlibs();

        let (tx, rx): (Sender<ExecState>, Receiver<ExecState>) = mpsc::channel();

        thread::spawn(move || {
            let mut lua = Lua::new();
            lua.openlibs();

            while let Ok(state) = rx.recv() {
                if let Some(delta) = Self::exec_script(&mut lua, state) {
                    delta_tx.send(delta).unwrap()
                }
            }
        });

        Interpreter { tx: tx }
    }

    pub fn exec(&mut self, script: &str, unit: &Unit, other: Option<&Unit>) {
        let other_snapshot = match other {
            Some(other_unit) => Some(UnitSnapshot::new(other_unit)),
            None => None,
        };
        self.tx.send((script.to_string(), UnitSnapshot::new(unit), other_snapshot)).unwrap();
    }

    fn exec_script(lua: &mut Lua, state: ExecState) -> Option<Delta> {
        let (script, self_unit, other_unit) = state;
        Self::set_unit(lua, "self", &self_unit);

        if let Some(other) = other_unit {
            Self::set_unit(lua, "other", &other)
        }

        lua.execute::<()>(&script).unwrap();

        let mut new_self: LuaTable<_> = lua.get("self").unwrap();
        let new_state: String = new_self.get("state").unwrap();

        match UnitState::from_str(&new_state) {
            Ok(state) => {
                if state != self_unit.state {
                    Some(Delta::new(self_unit.id, state))
                } else {
                    None
                }
            }
            Err(_) => panic!("Invalid state: {}", new_state),
        }
    }

    fn set_unit(lua: &mut Lua, index: &str, unit: &UnitSnapshot) {
        let mut table: LuaTable<_> = lua.empty_array(index);

        table.set("id", unit.id.hyphenated().to_string());
        table.set("x", unit.x);
        table.set("y", unit.y);
        table.set("team", unit.team as u32);
        table.set("role", unit.role.to_string());
        table.set("state", unit.state.to_string());
    }
}
