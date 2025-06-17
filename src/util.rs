use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use toml::Value;
use walkdir::WalkDir;

use anyhow::*;

pub fn cp_r(src: &Path, dst: &Path) -> Result<()> {
    for e in WalkDir::new(src) {
        // This is only an error when there's some sort of intermittent IO error
        // during iteration.
        // see https://doc.rust-lang.org/std/fs/struct.ReadDir.html
        let e = e.map_err(|_| {
            anyhow!(
                "intermittent IO error while iterating directory `{}`",
                src.display()
            )
        })?;

        let src_file = e.path();
        let relative_path = src_file.strip_prefix(src).map_err(|_| {
            anyhow!(
                "Could not retrieve relative path of child directory or \
                 file `{}` with regards to parent directory `{}`",
                src_file.display(),
                src.display()
            )
        })?;

        let dst_file = dst.join(relative_path);
        let metadata = e
            .metadata()
            .map_err(|e| anyhow!("Could not retrieve metadata\n{e:?}"))?;

        if metadata.is_dir() {
            // ensure the destination directory exists
            fs::create_dir_all(&dst_file).map_err(|e| {
                anyhow!("Could not create directory `{}`\n{e:?}", dst_file.display())
            })?;
        } else {
            // else copy the file
            fs::copy(&src_file, &dst_file).map_err(|_| {
                anyhow!(
                    "copying files from `{}` to `{}` failed",
                    src_file.display(),
                    dst_file.display()
                )
            })?;
        };
    }

    Ok(())
}

pub fn mkdir(path: &Path) -> Result<()> {
    fs::create_dir(path).map_err(|e| anyhow!("couldn't create directory {}\n{e:?}", path.display()))
}

/// Parses `path` as TOML
pub fn parse(path: &Path) -> Result<Value> {
    Ok(toml::from_str(&read(path)?)
        .map_err(|e| anyhow!("{} is not valid TOML\n{e:?}", path.display()))?)
}

pub fn read(path: &Path) -> Result<String> {
    let mut s = String::new();

    let p = path.display();
    File::open(path)
        .map_err(|e| anyhow!("couldn't open {}\n{e:?}", p))?
        .read_to_string(&mut s)
        .map_err(|e| anyhow!("couldn't read {}\n{e:?}", p))?;

    Ok(s)
}

/// Search for `file` in `path` and its parent directories
pub fn search<'p>(mut path: &'p Path, file: &str) -> Option<&'p Path> {
    loop {
        if path.join(file).exists() {
            return Some(path);
        }

        if let Some(p) = path.parent() {
            path = p;
        } else {
            return None;
        }
    }
}

pub fn write(path: &Path, contents: &str) -> Result<()> {
    let p = path.display();
    File::create(path)
        .map_err(|e| anyhow!("couldn't open {}\n{e:?}", p))?
        .write_all(contents.as_bytes())
        .map_err(|e| anyhow!("couldn't write to {}\n{e:?}", p))
}
