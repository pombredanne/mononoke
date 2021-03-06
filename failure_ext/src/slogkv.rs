// Copyright 2004-present Facebook. All Rights Reserved.

use super::Error;
use slog;

pub struct SlogKVError(pub Error);

impl slog::KV for SlogKVError {
    fn serialize(&self, _record: &slog::Record, serializer: &mut slog::Serializer) -> slog::Result {
        let err = &self.0;

        serializer.emit_str(Error.to_str(), &format!("{}", err))?;
        serializer.emit_str(RootCause.to_str(), &format!("{:#?}", err.find_root_cause()))?;
        serializer.emit_str(Backtrace.to_str(), &format!("{:#?}", err.backtrace()))?;

        for c in err.iter_chain().skip(1) {
            serializer.emit_str(Cause.to_str(), &format!("{}", c))?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SlogKVErrorKey {
    Error,
    RootCause,
    Backtrace,
    Cause,
}
use SlogKVErrorKey::*;

impl SlogKVErrorKey {
    pub fn to_str(self) -> &'static str {
        match self {
            Error => "error",
            RootCause => "root_cause",
            Backtrace => "backtrace",
            Cause => "cause",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "error" => Some(Error),
            "root_cause" => Some(RootCause),
            "backtrace" => Some(Backtrace),
            "cause" => Some(Cause),
            _ => None,
        }
    }
}
