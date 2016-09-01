use regex::Regex;
use std::str::FromStr;
use uuid::Uuid;

pub type Id = Uuid;

#[derive(Debug)]
pub enum TokenType {
    Float,
    Function,
    Id,
    Int,
    Other,
    Rest,
    Symbol,
    Tuple,
}

pub type Error = (String, TokenType);

pub fn read_fn(s: &str) -> Result<(&str, &str), Error> {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r"^([a-zA-Z0-9_]+)\("
        ).unwrap();
    }

    parse_str(&RE, s, TokenType::Function)
}

pub fn read_tuple(s: &str) -> Result<&str, Error> {
    if s.starts_with('(') {
        Ok(&s[1..])
    } else {
        Err((String::from(s), TokenType::Tuple))
    }
}

pub fn read_id(s: &str) -> Result<(Id, &str), Error> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"^([0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})",
            r"(, |\))?"
        )).unwrap();
    }

    match parse_str(&RE, s, TokenType::Id) {
        Ok((id_str, rest)) => {
            if let Ok(id) = Id::parse_str(id_str) {
                return Ok((id, rest));
            }
            Err((String::from(s), TokenType::Id))
        }
        Err(e) => Err(e),
    }
}

pub fn read_float(s: &str) -> Result<(f64, &str), Error> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"^(\d+.\d+)",
            r"(, |\))?"
        )).unwrap();
    }

    match parse_str(&RE, s, TokenType::Float) {
        Ok((f_str, rest)) => {
            if let Ok(u) = f64::from_str(f_str) {
                return Ok((u, rest));
            }
            Err((String::from(s), TokenType::Float))
        }
        Err(e) => Err(e),
    }
}

pub fn read_int(s: &str) -> Result<(usize, &str), Error> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"^(\d+)",
            r"(, |\))?"
        )).unwrap();
    }

    match parse_str(&RE, s, TokenType::Int) {
        Ok((u_str, rest)) => {
            if let Ok(u) = usize::from_str(u_str) {
                return Ok((u, rest));
            }
            Err((String::from(s), TokenType::Int))
        }
        Err(e) => Err(e),
    }
}

pub fn read_rest(s: &str) -> Result<(&str, &str), Error> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"^(.*)",
            r"\)?"
        )).unwrap();
    }

    parse_str(&RE, s, TokenType::Rest)
}

pub fn read_symbol(s: &str) -> Result<(&str, &str), Error> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"^([a-zA-Z0-9_-]+)",
            r"(, |\))?"
        )).unwrap();
    }

    parse_str(&RE, s, TokenType::Symbol)
}

fn parse_str<'a>(re: &'a Regex, s: &'a str, t: TokenType) -> Result<(&'a str, &'a str), Error> {
    if let Some(caps) = re.captures(s) {
        let start = caps.pos(1).unwrap().0;
        return Ok((&s[start..caps[1].len()], &s[caps[0].len()..]));
    };

    Err((String::from(s), t))
}
