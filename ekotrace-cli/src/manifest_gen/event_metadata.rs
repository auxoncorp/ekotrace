use crate::manifest_gen::source_location::SourceLocation;
use crate::manifest_gen::type_hint::TypeHint;
use std::fmt;

/// Event payload type hint and token
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Payload(pub TypeHint, pub String);

/// Event metadata
///
/// Events with payloads will have a `payload` field.
/// Events that have already been assigned an identifier will
/// have `assigned_id` set.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct EventMetadata {
    pub name: String,
    pub ekotrace_instance: String,
    pub payload: Option<Payload>,
    pub assigned_id: Option<u32>,
    pub location: SourceLocation,
}

impl EventMetadata {
    pub fn canonical_name(&self) -> String {
        self.name.to_lowercase()
    }
}

impl From<(TypeHint, String)> for Payload {
    fn from(triple: (TypeHint, String)) -> Payload {
        Payload(triple.0, triple.1)
    }
}

impl From<(TypeHint, &str)> for Payload {
    fn from(triple: (TypeHint, &str)) -> Payload {
        Payload(triple.0, triple.1.to_string())
    }
}

impl fmt::Display for EventMetadata {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Name: '{}'", self.canonical_name())?;
        writeln!(f, "Ekotrace instance: '{}'", self.ekotrace_instance)?;
        write!(f, "Payload type: ")?;
        match &self.payload {
            None => writeln!(f, "None")?,
            Some(p) => writeln!(f, "{}", p)?,
        }
        write!(f, "Assigned ID: ")?;
        match &self.assigned_id {
            None => writeln!(f, "None")?,
            Some(id) => writeln!(f, "{}", id)?,
        }
        write!(f, "{}", self.location)
    }
}

impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, '{}')", self.0.as_str(), self.1,)
    }
}
