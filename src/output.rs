use std::io::{self, Write};

use anyhow::Result;
use serde::Serialize;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Output {
    pub(crate) json: bool,
}

impl Output {
    pub(crate) fn json<T: Serialize>(&self, value: &T) -> Result<()> {
        serde_json::to_writer_pretty(io::stdout().lock(), value)?;
        println!();
        Ok(())
    }

    pub(crate) fn stdout(&self, value: &str) -> Result<()> {
        io::stdout().lock().write_all(value.as_bytes())?;
        Ok(())
    }

    pub(crate) fn stderr(&self, value: &str) -> Result<()> {
        io::stderr().lock().write_all(value.as_bytes())?;
        Ok(())
    }
}
