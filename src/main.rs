#![warn(clippy::pedantic)]

use std::{
    fs::{self, DirEntry, FileType}, io, path::{Path, PathBuf}, process::ExitCode
};

use clap::Parser;
use dialoguer::{Confirm, Error, Input};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help=true)]
struct Cli {
    /// Attempts to create a link for each file directly under base
    #[arg(value_parser = exists)]
    base: PathBuf,

    #[arg(value_parser = is_dir)]
    /// Target directory to write hardlinks to
    target: PathBuf,

    #[arg(short, long)]
    /// Use symbolic links instead of hard links. Will usually fail on Windows since creating symlinks is a privileged action.
    symbolic: bool,
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
    fn link_function<P: AsRef<Path>, Q: AsRef<Path>>(&self) -> fn(P, Q) -> io::Result<()> {
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

#[derive(PartialEq)]
enum ShouldExit {
    No,
    Yes,
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
    assert!(link.file_name().is_some(), "`link` didn't contain a file name. `link`: {}", link.display());
    let link_file_name = link.file_name().unwrap();

    let create_link = Confirm::new()
        .with_prompt(format!("Create link from {} to {}?", link.display(), original.display()))
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

/// Gets the file type of a directory entry. Follows symbolic links and will therefore never return a link file type.
fn get_definitive_file_type(entry: &DirEntry) -> io::Result<FileType> {
    Ok(fs::metadata(entry.path())?.file_type())
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

    for maybe_dir in dirs {
        let dir = match maybe_dir {
            Ok(dir) => dir,
            Err(err) => {
                eprintln!("Failed to open read dir: {err}");
                continue;
            },
        };

        let file_type = match get_definitive_file_type(&dir) {
            Ok(file_type) => file_type,
            Err(err) => {
                eprintln!("Failed to get entry file type: {err}");
                continue;
            },
        };

        if file_type.is_dir() {
            continue;
        }

        match link_file(&dir.path(), &cli.target.join(dir.file_name()), &cli) {
            Ok(ShouldExit::No) => {},
            Ok(ShouldExit::Yes) => break,
            Err(err) => {
                eprintln!("Encountered error while trying to link file: {err}");
                continue;
            },
        }
    }

    ExitCode::SUCCESS
}
