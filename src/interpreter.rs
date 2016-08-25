use hlua::{self, Lua, LuaTable};
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

pub struct UnitSnapshot {
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

pub type ExecState = (String, UnitSnapshot, Option<UnitSnapshot>);

#[derive(Debug)]
pub enum Error {
    DeltaChannelClosed(mpsc::SendError<ExecState>),
    LuaException(hlua::LuaError),
    LuaMissingKey(String),
}

impl From<hlua::LuaError> for Error {
    fn from(err: hlua::LuaError) -> Error {
        Error::LuaException(err)
    }
}

impl From<mpsc::SendError<ExecState>> for Error {
    fn from(err: mpsc::SendError<ExecState>) -> Error {
        Error::DeltaChannelClosed(err)
    }
}

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
                match Self::exec_script(&mut lua, state) {
                    Ok(Some(delta)) => delta_tx.send(delta).unwrap(),
                    Ok(None) => {},
                    Err(err) => panic!(err)
                }
            }
        });

        Interpreter { tx: tx }
    }

    pub fn exec(&mut self, script: &str, unit: &Unit, other: Option<&Unit>) -> Result<(), Error> {
        try!(self.tx
            .send((script.to_string(), UnitSnapshot::new(unit), other.map(UnitSnapshot::new))));
        Ok(())
    }

    fn exec_script(lua: &mut Lua, state: ExecState) -> Result<Option<Delta>, Error> {
        let (script, self_unit, other_unit) = state;
        Self::set_unit(lua, "self", &self_unit);

        if let Some(other) = other_unit {
            Self::set_unit(lua, "other", &other)
        }

        try!(lua.execute::<()>(&script));

        let mut new_self: LuaTable<_> = match lua.get("self") {
            Some(table) => table,
            None => return Err(Error::LuaMissingKey("self".to_string()))
        };

        let new_state: String = match new_self.get("state") {
            Some(state) => state,
            None => return Err(Error::LuaMissingKey("self.state".to_string()))
        };

        match UnitState::from_str(&new_state) {
            Ok(state) => {
                if state != self_unit.state {
                    Ok(Some(Delta::new(self_unit.id, state)))
                } else {
                    Ok(None)
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
