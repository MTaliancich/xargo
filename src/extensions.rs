use std::io;
use std::io::Write;
use std::process::{Command, ExitStatus};

use anyhow::*;

pub trait CommandExt {
    fn run(&mut self, verbose: bool) -> Result<()>;
    fn run_and_get_status(&mut self, verbose: bool) -> Result<ExitStatus>;
    fn run_and_get_stdout(&mut self, verbose: bool) -> Result<String>;
}

impl CommandExt for Command {
    /// Runs the command to completion
    fn run(&mut self, verbose: bool) -> Result<()> {
        let status = self.run_and_get_status(verbose)?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "`{:?}` failed with exit code: {:?}",
                self,
                status.code()
            ))?
        }
    }

    /// Runs the command to completion
    fn run_and_get_status(&mut self, verbose: bool) -> Result<ExitStatus> {
        if verbose {
            writeln!(io::stderr(), "+ {:?}", self).ok();
        }

        self.status()
            .map_err(|e| anyhow!("couldn't execute `{:?}`\n{e:?}", self))
    }

    /// Runs the command to completion and returns its stdout
    fn run_and_get_stdout(&mut self, verbose: bool) -> Result<String> {
        if verbose {
            writeln!(io::stderr(), "+ {:?}", self).ok();
        }

        let out = self
            .output()
            .map_err(|e| anyhow!("couldn't execute `{:?}`\n{e:?}", self))?;

        if out.status.success() {
            Ok(String::from_utf8(out.stdout)
                .map_err(|e| anyhow!("`{:?}` output was not UTF-8\n{e:?}", self))?)
        } else {
            Err(anyhow!(
                "`{:?}` failed with exit code: {:?}",
                self,
                out.status.code()
            ))?
        }
    }
}
