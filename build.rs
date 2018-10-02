use fs::File;
use io::BufRead;
use io::Read;
use os::unix::fs::OpenOptionsExt;
use path::{Path, PathBuf};
use std::{env, fmt, fs, io, os, path};

enum Error {
    GitDirNotFound,
    Io(io::Error),
    OutDir(env::VarError),
}

type Result<T> = std::result::Result<T, Error>;

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Io(error)
    }
}

impl From<env::VarError> for Error {
    fn from(error: env::VarError) -> Error {
        Error::OutDir(error)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            Error::GitDirNotFound => format!(
                ".git directory was not found in '{}' or its parent directories",
                env::var("OUT_DIR").unwrap_or("".to_string()),
            ),
            Error::Io(inner) => format!("IO error: {}", inner),
            Error::OutDir(env::VarError::NotPresent) => unreachable!(),
            Error::OutDir(env::VarError::NotUnicode(msg)) => msg.to_string_lossy().to_string(),
        };
        write!(f, "{}", msg)
    }
}

fn resolve_gitdir() -> Result<PathBuf> {
    let dir = env::var("OUT_DIR")?;
    let mut dir = PathBuf::from(dir);
    if !dir.has_root() {
        dir = fs::canonicalize(dir)?;
    }
    loop {
        let gitdir = dir.join(".git");
        if gitdir.is_dir() {
            return Ok(gitdir);
        }
        if gitdir.is_file() {
            let mut buf = String::new();
            File::open(gitdir)?.read_to_string(&mut buf)?;
            let gitdir = PathBuf::from(buf.trim_right_matches("\n\r"));
            if !gitdir.is_dir() {
                return Err(Error::GitDirNotFound);
            }
            return Ok(gitdir);
        }
        if !dir.pop() {
            return Err(Error::GitDirNotFound);
        }
    }
}

fn hook_already_exists(hook: &Path) -> bool {
    let f = match File::open(hook) {
        Ok(f) => f,
        Err(..) => return false,
    };
    match io::BufReader::new(f).lines().nth(2) {
        None | Some(Err(..)) => false,
        Some(Ok(line)) => {
            let ver_comment = format!("set by cargo-husky v{}", env!("CARGO_PKG_VERSION"));
            line.contains(&ver_comment)
        }
    }
}

fn write_script<W: io::Write>(w: &mut W) -> Result<()> {
    let script = {
        let mut s = String::new();
        if cfg!(feature = "run-cargo-test") {
            s += "\necho '+cargo test'\ncargo test";
        }
        if cfg!(feature = "run-cargo-clippy") {
            s += "\necho '+cargo clippy'\ncargo clippy";
        }
        s
    };

    writeln!(
        w,
        r#"#!/bin/sh
#
# This hook was set by cargo-husky v{}: {}
# Generated by script {}{}build.rs
# Output at {}
#

set -e
{}"#,
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_HOMEPAGE"),
        env!("CARGO_MANIFEST_DIR"),
        path::MAIN_SEPARATOR,
        env::var("OUT_DIR").unwrap_or("".to_string()),
        script
    )?;
    Ok(())
}

#[cfg(target_os = "win32")]
fn create_script(path: &Path) -> io::Result<File> {
    fs::create(path)
}

#[cfg(not(target_os = "win32"))]
fn create_script(path: &Path) -> io::Result<File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o755)
        .open(path)
}

fn install(hook: &str) -> Result<()> {
    let hook_path = {
        let mut p = resolve_gitdir()?;
        p.push("hooks");
        p.push(hook);
        p
    };
    if !hook_already_exists(hook_path.as_path()) {
        let mut f = create_script(hook_path.as_path())?;
        write_script(&mut f)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    if cfg!(feature = "prepush-hook") {
        install("pre-push")?;
    }
    if cfg!(feature = "precommit-hook") {
        install("pre-commit")?;
    }
    if cfg!(feature = "postmerge-hook") {
        install("post-merge")?;
    }
    Ok(())
}
