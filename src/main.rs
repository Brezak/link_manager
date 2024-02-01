#![warn(clippy::pedantic)]

use std::{
    fs::{self, DirEntry, FileType, ReadDir}, io, path::{Path, PathBuf}, process::ExitCode
};

use clap::Parser;
use dialoguer::{Confirm, Error, Input};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help=true)]
struct Cli {
    /// Attempts to create a link for each file under base
    #[arg(value_parser = exists)]
    base: PathBuf,

    #[arg(value_parser = is_dir)]
    /// Target directory to write hardlinks to
    target: PathBuf,

    #[arg(short, long)]
    /// Use symbolic links instead of hard links. Will usually fail on Windows since creating symlinks is a privileged action.
    symbolic: bool,

    #[arg(short, long)]
    /// Recurse into directories while creating symlinks
    recurse: bool
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
    
    let metadata = path.metadata().map_err(|err| format!("Can't open <TARGET> directory: {err}"))?;
    if metadata.is_dir() {
        Ok(path)
    } else {
        Err("<TARGET> is not a directory!".to_string())
    }
}

impl Cli {
    const fn link_function<P: AsRef<Path>, Q: AsRef<Path>>(&self) -> fn(P, Q) -> io::Result<()> {
        #[cfg(target_family = "unix")]
        if self.symbolic {
            return std::os::unix::fs::symlink
        }
        #[cfg(target_family = "windows")]
        if self.symbolic {
            return std::os::windows::fs::symlink_file;
        }

        fs::hard_link
    }
}

enum ShouldExit {
    No,
    Yes,
}

impl ShouldExit {
    const fn should_exit(&self) -> bool {
        matches!(self, Self::Yes)
    }
}

/// Prompts the user to create a link and creates one if they agree.
/// 
/// `original` File to create a link to.
/// `link` Link that will point to `original`
/// 
/// # Panics
/// 
/// When link doesn't contain a filename.
/// 
fn link_file(original: &Path, link: &Path, cli: &Cli) -> io::Result<ShouldExit> {
    let maybe_link_name = link.file_name();
    assert!(maybe_link_name.is_some(), "`link` didn't contain a file name. `link`: {}", link.display());
    let link_file_name = maybe_link_name.unwrap();

    let create_link = Confirm::new()
        .with_prompt(format!("Create link from {} to {}?", link.display(), original.display()))
        .default(true)
        .interact_opt()
        .map_err(|Error::IO(err)| err)?;

    let Some(create_link) = create_link else {
        return Ok(ShouldExit::Yes);
    };

    if !create_link {
        return Ok(ShouldExit::No);
    }

    let link_file_name: String = Input::new()
        .with_prompt("Link name")
        .with_initial_text(link_file_name.to_string_lossy())
        .interact_text() // For some reason supports utf-8
        .map_err(|Error::IO(err)| err)?;

    let mut link = link.to_path_buf();
    link.set_file_name(link_file_name);
    let link_function = cli.link_function();
    link_function(original, link)?;

    Ok(ShouldExit::No)
}

enum CreateDirContinuation {
    Exit,
    Continue,
    MaybeRecurse(PathBuf),
}

fn create_dir(location: &Path, name: &Path) -> io::Result<CreateDirContinuation> {
    let create = Confirm::new()
        .with_prompt(format!("Recreate the {} directory in {}", name.display(), location.display()))
        .default(true)
        .interact_opt()
        .map_err(|Error::IO(err)| err)?;

    let Some(create) = create else {
        return Ok(CreateDirContinuation::Exit)
    };

    if !create {
        return Ok(CreateDirContinuation::Continue);
    }

    let dir_name: String = Input::new()
        .with_prompt("Dir name")
        .with_initial_text(name.to_string_lossy())
        .interact_text()
        .map_err(|Error::IO(err)| err)?;

    let new_dir_path = location.join(dir_name);

    fs::create_dir(&new_dir_path)?;

    Ok(CreateDirContinuation::MaybeRecurse(new_dir_path))
}

/// Gets the file type of a directory entry. Follows symbolic links and will therefore never return a link file type.
fn get_definitive_file_type(entry: &DirEntry) -> io::Result<FileType> {
    Ok(fs::metadata(entry.path())?.file_type())
}

fn recurse_into_dir(directory: ReadDir, target: &Path, cli: &Cli) -> ShouldExit {
    for maybe_dir in directory {
        let entry = match maybe_dir {
            Ok(dir) => dir,
            Err(err) => {
                eprintln!("Failed to open read dir: {err}");
                continue;
            },
        };

        let file_type = match get_definitive_file_type(&entry) {
            Ok(file_type) => file_type,
            Err(err) => {
                eprintln!("Failed to get entry file type: {err}");
                continue;
            },
        };

        if file_type.is_file() {
            match link_file(&entry.path(), &target.join(entry.file_name()), cli) {
                Ok(ShouldExit::No) => continue,
                Ok(ShouldExit::Yes) => return ShouldExit::Yes,
                Err(err) => {
                    eprintln!("Encountered error while trying to link file: {err}");
                    continue;
                },
            }
        }
        
        match create_dir(target, Path::new(&entry.file_name())) {
            Ok(CreateDirContinuation::Exit) => return ShouldExit::Yes,
            Ok(CreateDirContinuation::Continue) => continue,
            Ok(CreateDirContinuation::MaybeRecurse(new_dir_path)) => {
                if !cli.recurse {
                    continue;
                }

                let recurse = Confirm::new()
                    .with_prompt("Should we recurse into the recreated folder?")
                    .default(true)
                    .interact_opt()
                    .map_err(|Error::IO(err)| err);

                if let Err(err) = recurse {
                    eprintln!("Error in prompt: {err}");
                    continue;
                }

                let recurse = recurse.unwrap();
                if recurse.is_none() {
                    return ShouldExit::Yes;
                }

                if !recurse.unwrap() {
                    continue;
                }

                let recurse_dirs = match entry.path().read_dir() {
                    Ok(recurse_dirs) => recurse_dirs,
                    Err(err) => {
                        eprintln!("Failed to recurse into directory: {err}");
                        continue;
                    },
                };

                if recurse_into_dir(recurse_dirs, &new_dir_path, cli).should_exit() {
                    return ShouldExit::Yes;
                }
            }
            Err(err) => {
                eprintln!("Failed to create file: {err}");
                continue;
            },
        }
    }

    ShouldExit::No
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.base.is_file() {
        let base_file_name = cli.base.file_name().expect("<BASE> was provided a file that doesn't have a valid filename by Rust rules");
        let link = cli.target.join(base_file_name); // We have validated target to be a directory.
        if let Err(err) = link_file(&cli.base, &link, &cli) {
            eprintln!("Encountered and error while handling file: {err}");
            return ExitCode::FAILURE;
        }

        return ExitCode::SUCCESS;
    }

    let dirs = match cli.base.read_dir() {
        Ok(dirs) => dirs,
        Err(err) => {
            eprintln!("Failed to read <BASE> dir: {err}");
            return ExitCode::FAILURE;
        },
    };

    recurse_into_dir(dirs, &cli.target, &cli);

    ExitCode::SUCCESS
}
