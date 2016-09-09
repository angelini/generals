use hlua::{self, Lua, LuaTable};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time;

use parser::{self, TokenType};
use unit::{Id, Unit, UnitRole, UnitState};

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
pub enum Delta {
    UpdateState(Id, UnitState),
    NewUnit(UnitRole, Id, f64, f64, f64, usize),
}

impl FromStr for Delta {
    type Err = parser::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parser::read_fn(s) {
            Ok(("new_unit", s)) => {
                let (role, s) = try!(parser::read_symbol(s));
                let (id, s) = try!(parser::read_id(s));
                let (x, s) = try!(parser::read_float(s));
                let (y, s) = try!(parser::read_float(s));
                let (rotation, s) = try!(parser::read_float(s));
                let (team, _) = try!(parser::read_int(s));

                match UnitRole::from_str(role) {
                    Ok(r) => Ok(Delta::NewUnit(r, id, x, y, rotation, team)),
                    Err(string) => Err((string, TokenType::Other)),
                }
            }
            Ok(("update_state", s)) => {
                let (id, s) = try!(parser::read_id(s));
                let (state, _) = try!(parser::read_rest(s));

                match UnitState::from_str(state) {
                    Ok(s) => Ok(Delta::UpdateState(id, s)),
                    Err(e) => Err(e),
                }
            }
            _ => Err((String::from(s), TokenType::Other)),
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
        let (x, y) = unit.xy();
        UnitSnapshot {
            id: unit.id,
            x: x,
            y: y,
            team: unit.team,
            role: unit.role,
            state: unit.state.clone(),
        }
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

fn gen_uuid() -> String {
    Id::new_v4().hyphenated().to_string()
}

#[derive(Debug)]
struct TimelineEvent {
    time: usize,
    delta: Delta,
}

impl FromStr for TimelineEvent {
    type Err = parser::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parser::read_tuple(s) {
            Ok(s) => {
                let (time, s) = try!(parser::read_int(s));
                let (delta, _) = try!(parser::read_rest(s));

                match Delta::from_str(delta) {
                    Ok(d) => {
                        Ok(TimelineEvent {
                            time: time,
                            delta: d,
                        })
                    }
                    Err(e) => Err(e),
                }
            }
            _ => Err((String::from(s), TokenType::Other)),
        }
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
        let delta_tx_cloned = delta_tx.clone();

        thread::spawn(move || {
            let mut lua = Self::new_lua_instance();

            let mut timeline = match Self::generate_timeline(&mut lua) {
                Ok(events) => events,
                Err(err) => panic!(err),
            };
            timeline.sort_by(|l, r| l.time.cmp(&r.time));

            let mut current_time = 0;
            for event in timeline {
                let wait_time = event.time - current_time;
                let duration = time::Duration::from_secs(wait_time as u64);

                info!(target: "timeline", "waiting {}", wait_time);
                thread::sleep(duration);

                current_time += wait_time;
                info!(target: "timeline", "{:?}", event);
                delta_tx_cloned.send(event.delta).unwrap();
            }
        });

        thread::spawn(move || {
            let mut lua = Self::new_lua_instance();

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
                try!(lua.execute(&format!("__result = {}(__self, __other)", function)));
            }
            None => try!(lua.execute(&format!("__result = {}(__self)", function))),
        }

        let new_state: String = match lua.get("__result") {
            Some(state) => state,
            None => return Ok(None),
        };

        match UnitState::from_str(&new_state) {
            Ok(state) => {
                if state != self_unit.state {
                    Ok(Some(Delta::UpdateState(self_unit.id, state)))
                } else {
                    Ok(None)
                }
            }
            Err(_) => panic!("Invalid state: {}", new_state),
        }
    }

    fn generate_timeline(lua: &mut Lua) -> Result<Vec<TimelineEvent>, Error> {
        if try!(lua.execute::<bool>("return _G[\"timeline\"] == nil")) {
            return Ok(vec![]);
        }

        try!(lua.execute("__timeline = __flatten_timeline(timeline())"));

        let mut timeline: LuaTable<_> = match lua.get("__timeline") {
            Some(table) => table,
            None => return Err(Error::LuaIndexNotFound("__timeline".to_string())),
        };

        let result = timeline.iter()
            .filter_map(|e| e)
            .map(|(_, v): (u32, String)| TimelineEvent::from_str(&v).unwrap())
            .collect::<Vec<TimelineEvent>>();
        Ok(result)
    }

    fn new_lua_instance<'a>() -> Lua<'a> {
        let mut lua = Lua::new();
        lua.openlibs();
        lua.set("uuid", hlua::function0(gen_uuid));
        match load_lua_scripts(&mut lua) {
            Ok(_) => lua,
            Err(err) => panic!(err),
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
