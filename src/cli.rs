use std::{fs, io, path::{Path, PathBuf}};

use clap::{Parser, ValueEnum};
use clap_complete::Shell;


#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Action {
    Always,
    Ask,
    Never,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help = true)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    /// Attempts to create a link for each file under base
    #[arg(value_parser = exists)]
    pub base: PathBuf,

    #[arg(value_parser = is_dir)]
    /// Target directory to write hardlinks to
    pub target: PathBuf,

    #[arg(short, long)]
    /// Use symbolic links instead of hard links. Will usually fail on Windows since creating symlinks is a privileged action
    symbolic: bool,

    #[arg(short = 'f', long)]
    /// Always create directories/links, never rename directories/links, always recurse. Each actionn can be overriden by more specific flags
    never_prompt: bool,

    #[arg(long)]
    /// Always create links instead of prompting
    always_create_links: bool,

    #[arg(long)]
    /// How to handle dirs (Defaults to ask)
    create_dirs: Option<Action>,

    #[arg(long)]
    /// Recurse into directories while creating symlinks (Defaults to ask)
    recurse: Option<Action>,

    #[arg(long)]
    /// Prompt the user for a new name for a dir. This is the default behaviour and this flag is only usefull to override --never-prompt
    ask_to_rename_dirs: bool,

    #[arg(long)]
    /// Prompt the user for a new name for a link. This is the default behaviour and this flag is only usefull to override --never-prompt
    ask_to_rename_links: bool,

    #[arg(long)]
    /// Generate completions, print the to stdout, exit.
    completions: Option<Shell>,
}

fn exists(path: &str) -> Result<PathBuf, String> {
    let buf = PathBuf::from(path);

    if !buf.exists() {
        return Err("<BASE> path doesn't exist!".to_string());
    }

    Ok(buf)
}

fn is_dir(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);

    let metadata = path
        .metadata()
        .map_err(|err| format!("Can't open <TARGET> directory: {err}"))?;
    if metadata.is_dir() {
        Ok(path)
    } else {
        Err("<TARGET> is not a directory!".to_string())
    }
}

impl Cli {
    pub const fn link_function<P: AsRef<Path>, Q: AsRef<Path>>(&self) -> fn(P, Q) -> io::Result<()> {
        #[cfg(target_family = "unix")]
        if self.symbolic() {
            return std::os::unix::fs::symlink;
        }
        #[cfg(target_family = "windows")]
        if self.symbolic() {
            return std::os::windows::fs::symlink_file;
        }

        fs::hard_link
    }

    pub fn recurse(&self) -> Action {
        self.recurse.unwrap_or(if self.never_prompt {
            Action::Always
        } else {
            Action::Ask
        })
    }

    pub fn create_dirs(&self) -> Action {
        self.create_dirs.unwrap_or(if self.never_prompt {
            Action::Always
        } else {
            Action::Ask
        })
    }

    pub const fn ask_to_rename_dirs(&self) -> bool {
        !self.never_prompt || self.ask_to_rename_dirs
    }

    pub const fn create_links(&self) -> Action {
        if self.always_create_links || self.never_prompt {
            Action::Always
        } else {
            Action::Ask
        }
    }

    pub const fn ask_to_rename_links(&self) -> bool {
        !self.never_prompt || self.ask_to_rename_links
    }

    pub const fn symbolic(&self) -> bool {
        self.symbolic
    }

    pub fn completions(&self) -> Option<Shell> {
        self.completions
    }
}

pub enum ShouldExit {
    No,
    Yes,
}

impl ShouldExit {
    pub const fn should_exit(&self) -> bool {
        matches!(self, Self::Yes)
    }
}