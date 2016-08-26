use hlua::{self, Lua, LuaTable};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use unit::{Id, Unit, UnitRole, UnitState};

#[derive(Debug)]
pub enum EventType {
    Collision,
    EnterView,
    ExitView,
    StateChange,
}

impl ToString for EventType {
    fn to_string(&self) -> String {
        match *self {
            EventType::Collision => String::from("collision"),
            EventType::EnterView => String::from("enter_view"),
            EventType::ExitView => String::from("exit_view"),
            EventType::StateChange => String::from("state_change"),
        }
    }
}

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
    Io(io::Error),
    DeltaChannelClosed(mpsc::SendError<ExecState>),
    LuaException(hlua::LuaError),
    LuaIndexNotFound(String),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
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

fn read_dir(dir: &str) -> Result<Vec<String>, io::Error> {
    try!(fs::read_dir(dir))
        .map(|dir| dir.unwrap().path())
        .map(fs::File::open)
        .map(|file| {
            let mut s = String::new();
            try!(file).read_to_string(&mut s).unwrap();
            Ok(s)
        })
        .collect::<Result<Vec<String>, io::Error>>()
}

fn load_lua_scripts(lua: &mut Lua) -> Result<(), Error> {
    for script in &try!(read_dir("./lua")) {
        try!(lua.execute::<()>(script));
    }
    Ok(())
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

            match load_lua_scripts(&mut lua) {
                Ok(_) => {}
                Err(err) => panic!(err),
            }

            while let Ok(state) = rx.recv() {
                match Self::exec_function(&mut lua, state) {
                    Ok(Some(delta)) => delta_tx.send(delta).unwrap(),
                    Ok(None) => {}
                    Err(err) => panic!(err),
                }
            }
        });

        Interpreter { tx: tx }
    }

    pub fn exec(&mut self,
                role: &UnitRole,
                event_type: &EventType,
                unit: &Unit,
                other: Option<&Unit>)
                -> Result<(), Error> {
        let function = format!("{}_on_{}", role.to_string(), event_type.to_string());
        try!(self.tx
            .send((function, UnitSnapshot::new(unit), other.map(UnitSnapshot::new))));
        Ok(())
    }

    fn exec_function(lua: &mut Lua, state: ExecState) -> Result<Option<Delta>, Error> {
        let (function, self_unit, other_unit) = state;

        if try!(lua.execute::<bool>(&format!("return _G[\"{}\"] == nil", function))) {
            return Ok(None);
        }

        Self::set_unit(lua, "__self", &self_unit);

        match other_unit {
            Some(other) => {
                Self::set_unit(lua, "__other", &other);
                try!(lua.execute(&format!("{}(__self, __other)", function)));
            }
            None => try!(lua.execute(&format!("{}(__self)", function))),
        }

        let mut new_self: LuaTable<_> = match lua.get("__self") {
            Some(table) => table,
            None => return Err(Error::LuaIndexNotFound("__self".to_string())),
        };

        let new_state: String = match new_self.get("state") {
            Some(state) => state,
            None => return Err(Error::LuaIndexNotFound("__self.state".to_string())),
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
