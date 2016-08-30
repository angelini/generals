use hlua::{self, Lua, LuaTable};
use regex::Regex;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time;

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
pub enum Delta {
    UpdateState(Id, UnitState),
    NewUnit(UnitRole, Id, f64, f64, f64, usize),
}

impl FromStr for Delta {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(concat!(
            r"new_unit\(",
            r"(?P<role>soldier|general|bullet)",
            r", ",
            r"(?P<id>[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})",
            r", ",
            r"(?P<x>\d+.\d+)",
            r", ",
            r"(?P<y>\d+.\d+)",
            r", ",
            r"(?P<rotation>\d+.\d+)",
            r", ",
            r"(?P<team>\d+)",
            r"\)"))
            .unwrap();
        if let Some(caps) = re.captures(s) {
            let role = match caps.name("role").unwrap() {
                "soldier" => UnitRole::Soldier,
                "general" => UnitRole::General,
                "bullet" => UnitRole::Bullet,
                _ => panic!("invalid regex state"),
            };
            let id = Id::parse_str(caps.name("id").unwrap()).unwrap();
            let x = f64::from_str(caps.name("x").unwrap()).unwrap();
            let y = f64::from_str(caps.name("y").unwrap()).unwrap();
            let rotation = f64::from_str(caps.name("rotation").unwrap()).unwrap();
            let team = usize::from_str(caps.name("team").unwrap()).unwrap();
            return Ok(Delta::NewUnit(role, id, x, y, rotation, team));
        };

        let re = Regex::new(concat!(
            r"update_state\(",
            r"(?P<id>[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})",
            r", ",
            r"(?P<state>.*)",
            r"\)"))
            .unwrap();
        if let Some(caps) = re.captures(s) {
            let id = Id::parse_str(caps.name("id").unwrap()).unwrap();
            let state = UnitState::from_str(caps.name("state").unwrap()).unwrap();
            return Ok(Delta::UpdateState(id, state));
        };

        Err(s.to_string())
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

fn gen_uuid() -> String {
    Id::new_v4().hyphenated().to_string()
}

#[derive(Debug)]
struct TimelineEvent {
    time: u32,
    delta: Delta,
}

impl FromStr for TimelineEvent {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"\((?P<time>\d+), (?P<delta>.*)\)").unwrap();
        match re.captures(s) {
            Some(caps) => {
                let time = u32::from_str(caps.name("time").unwrap()).unwrap();
                let delta = Delta::from_str(caps.name("delta").unwrap()).unwrap();
                Ok(TimelineEvent {
                    time: time,
                    delta: delta,
                })
            }
            None => Err(s.to_string()),
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
            let mut lua = Lua::new();
            lua.openlibs();

            lua.set("uuid", hlua::function0(gen_uuid));

            match load_lua_scripts(&mut lua) {
                Ok(_) => {}
                Err(err) => panic!(err),
            }

            let timeline = match Self::generate_timeline(&mut lua) {
                Ok(events) => events,
                Err(err) => panic!(err),
            };
            info!(target: "timeline", "{:?}", timeline);

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
            let mut lua = Lua::new();
            lua.openlibs();

            lua.set("uuid", hlua::function0(gen_uuid));

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

        try!(lua.execute("__timeline = timeline()"));

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
