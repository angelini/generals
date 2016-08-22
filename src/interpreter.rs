use hlua::{Lua, LuaTable};
use std::str::FromStr;

use unit::{Id, Unit, UnitState};

#[derive(Debug)]
pub enum Delta {
    StateChange(Id, UnitState),
    NewUnit(Unit),
}

pub struct Interpreter<'a> {
    lua: Lua<'a>,
}

impl<'a> Interpreter<'a> {
    pub fn new() -> Interpreter<'a> {
        let mut lua = Lua::new();
        lua.openlibs();
        Interpreter { lua: lua }
    }

    pub fn exec(&mut self, unit: &Unit, script: &str, other: Option<&Unit>) -> Vec<Delta> {
        self.set_unit("self", unit);

        if let Some(other_unit) = other {
            self.set_unit("other", other_unit);
        }

        self.lua.execute::<()>(script).unwrap();

        let mut new_self: LuaTable<_> = self.lua.get("self").unwrap();
        let new_state: String = new_self.get("state").unwrap();

        match UnitState::from_str(&new_state) {
            Ok(state) => {
                if state != unit.state {
                    vec![Delta::StateChange(unit.id, state)]
                } else {
                    vec![]
                }
            }
            Err(_) => panic!("Invalid state: {}", new_state),
        }
    }

    fn set_unit(&mut self, index: &str, unit: &Unit) {
        let mut table: LuaTable<_> = self.lua.empty_array(index);

        table.set("x", unit.x);
        table.set("y", unit.y);
        table.set("role", unit.role.to_string());
        table.set("state", unit.state.to_string());
    }
}
