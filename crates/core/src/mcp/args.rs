//! Reading a tool call's arguments, with messages an agent can act on.

use serde_json::Value;

use crate::error::Error;

/// The arguments of one tool call.
#[derive(Clone, Copy, Debug)]
pub struct Args<'a>(pub &'a Value);

impl<'a> Args<'a> {
    fn get(&self, name: &str) -> Option<&'a Value> {
        self.0.get(name).filter(|v| !v.is_null())
    }

    /// A required text.
    ///
    /// # Errors
    /// It is missing or is not text.
    pub fn text(&self, name: &str) -> Result<&'a str, Error> {
        self.opt_text(name)?.ok_or_else(|| Error::Agent(format!("\"{name}\" is required.")))
    }

    /// An optional text.
    ///
    /// # Errors
    /// It is there and is not text.
    pub fn opt_text(&self, name: &str) -> Result<Option<&'a str>, Error> {
        self.get(name).map(|v| v.as_str().ok_or_else(|| Error::Agent(format!("\"{name}\" must be text.")))).transpose()
    }

    /// An optional whole number, at least 0.
    ///
    /// # Errors
    /// It is there and is not such a number.
    pub fn number(&self, name: &str) -> Result<Option<u64>, Error> {
        self.get(name).map(|v| v.as_u64().ok_or_else(|| Error::Agent(format!("\"{name}\" must be a whole number.")))).transpose()
    }

    /// An optional yes or no, no when left out.
    ///
    /// # Errors
    /// It is there and is not true or false.
    pub fn flag(&self, name: &str) -> Result<bool, Error> {
        Ok(self.get(name).map(|v| v.as_bool().ok_or_else(|| Error::Agent(format!("\"{name}\" must be true or false.")))).transpose()?.unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn arguments_are_read_and_wrong_types_are_said() {
        let v = json!({ "a": "x", "n": 3, "yes": true, "none": null, "bad": -1 });
        let args = Args(&v);
        assert_eq!(args.text("a"), Ok("x"));
        assert_eq!(args.opt_text("missing"), Ok(None));
        assert_eq!(args.opt_text("none"), Ok(None));
        assert_eq!(args.number("n"), Ok(Some(3)));
        assert_eq!(args.flag("yes"), Ok(true));
        assert_eq!(args.flag("missing"), Ok(false));
        assert!(args.text("missing").is_err());
        assert!(args.text("n").is_err());
        assert!(args.number("bad").is_err());
        assert!(args.flag("a").is_err());
    }
}
