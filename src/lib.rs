#![deny(warnings)]

extern crate anyhow;
extern crate fs2;
extern crate home;
#[cfg(any(
    all(target_os = "linux", not(target_env = "musl")),
    target_os = "macos"
))]
extern crate libc;
extern crate rustc_version;
extern crate serde_json;
extern crate tempdir;
extern crate toml;
extern crate walkdir;

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;
use std::process::ExitStatus;
use std::result::Result::Ok;
use std::{env, io, process};

use rustc_version::Channel;

use crate::rustc::Target;
use anyhow::*;

mod cargo;
mod cli;
mod extensions;
mod flock;
mod rustc;
mod sysroot;
mod util;
mod xargo;

pub use crate::sysroot::XargoMode;

// We use a different sysroot for Native compilation to avoid file locking
//
// Cross compilation requires `lib/rustlib/$HOST` to match `rustc`'s sysroot,
// whereas Native compilation wants to use a custom `lib/rustlib/$HOST`. If each
// mode has its own sysroot then we avoid sharing that directory and thus file
// locking it.
pub enum CompilationMode {
    Cross(Target),
    Native(String),
}

impl CompilationMode {
    fn hash<H>(&self, hasher: &mut H) -> Result<()>
    where
        H: Hasher,
    {
        match *self {
            CompilationMode::Cross(ref target) => target.hash(hasher)?,
            CompilationMode::Native(ref triple) => triple.hash(hasher),
        }

        Ok(())
    }

    fn triple(&self) -> &str {
        match *self {
            CompilationMode::Cross(ref target) => target.triple(),
            CompilationMode::Native(ref triple) => triple,
        }
    }

    fn is_native(&self) -> bool {
        match *self {
            CompilationMode::Native(_) => true,
            _ => false,
        }
    }
}

pub fn main_inner(xargo_mode: XargoMode) {
    fn show_backtrace() -> bool {
        if let Ok(trace) = env::var("RUST_BACKTRACE").as_ref().map(|s| (s[..].trim())) {
            match trace {
                "1" | "full" => true,
                _ => false,
            }
        } else {
            false
        }
    }

    match run(xargo_mode) {
        Err(e) => {
            let stderr = io::stderr();
            let mut stderr = stderr.lock();

            writeln!(stderr, "error: {}", e).ok();

            if show_backtrace() {
                writeln!(stderr, "{:?}", e.backtrace()).ok();
            } else {
                writeln!(stderr, "note: run with `RUST_BACKTRACE=1` for a backtrace").ok();
            }

            process::exit(1)
        }
        Ok(Some(status)) => {
            if !status.success() {
                process::exit(status.code().unwrap_or(1))
            }
        }
        Ok(None) => {}
    }
}

fn run(cargo_mode: XargoMode) -> Result<Option<ExitStatus>> {
    let args = cli::args();
    let verbose = args.verbose();

    let meta = rustc::version().map_err(|e| anyhow!("could not determine rustc version\n{e:?}"))?;

    if let Some(sc) = args.subcommand() {
        if !sc.needs_sysroot() {
            return cargo::run(&args, verbose).map(Some);
        }
    } else if args.version() {
        writeln!(
            io::stderr(),
            concat!("xargo ", env!("CARGO_PKG_VERSION"), "{}"),
            include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))
        )
        .ok();

        return cargo::run(&args, verbose).map(Some);
    }

    let config = cargo::config()?;
    if let Some(root) = cargo::root(cargo_mode, args.manifest_path())? {
        // We can't build sysroot with stable or beta due to unstable features
        let sysroot = rustc::sysroot(verbose)?;
        let src = match meta.channel {
            Channel::Dev => rustc::Src::from_env().ok_or(anyhow!(
                "The XARGO_RUST_SRC env variable must be set and point to the \
                 Rust source directory when working with the 'dev' channel"
            ))?,
            Channel::Nightly => {
                if let Some(src) = rustc::Src::from_env() {
                    src
                } else {
                    sysroot.src()?
                }
            }
            Channel::Stable | Channel::Beta => {
                eprintln!(
                    "ERROR: the sysroot can't be built for the {:?} channel. \
                     Switch to nightly.",
                    meta.channel
                );
                process::exit(1);
            }
        };
        let cmode = if let Some(triple) = args.target() {
            if Path::new(triple).is_file() {
                bail!(
                    "Xargo doesn't support files as an argument to --target. \
                     Use `--target foo` instead of `--target foo.json`."
                )
            } else if triple == meta.host {
                Some(CompilationMode::Native(meta.host.clone()))
            } else {
                Target::new(triple, &root, verbose)?.map(CompilationMode::Cross)
            }
        } else {
            if let Some(ref config) = config {
                if let Some(triple) = config.target()? {
                    Target::new(triple, &root, verbose)?.map(CompilationMode::Cross)
                } else {
                    Some(CompilationMode::Native(meta.host.clone()))
                }
            } else {
                Some(CompilationMode::Native(meta.host.clone()))
            }
        };

        if let Some(cmode) = cmode {
            let home = xargo::home(&cmode)?;
            let rustflags = cargo::rustflags(config.as_ref(), cmode.triple())?;

            sysroot::update(
                &cmode,
                &home,
                &root,
                &rustflags,
                &meta,
                &src,
                &sysroot,
                verbose,
                args.message_format(),
                cargo_mode,
            )?;

            if args.subcommand().is_some() || cargo_mode == XargoMode::Build {
                return xargo::run(
                    &args,
                    &cmode,
                    rustflags,
                    &home,
                    &meta,
                    config.as_ref(),
                    verbose,
                )
                .map(Some);
            } else {
                return Ok(None);
            }
        }
    }

    cargo::run(&args, verbose).map(Some)
}
