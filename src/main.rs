#![warn(clippy::pedantic)]

mod cli;

use std::{
    fs::{self, DirEntry, FileType, ReadDir},
    io,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, ShouldExit, Action};
use dialoguer::{Confirm, Error, Input};

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
    assert!(
        maybe_link_name.is_some(),
        "`link` didn't contain a file name. `link`: {}",
        link.display()
    );
    let link_file_name = maybe_link_name.unwrap();

    let create_link = if cli.create_links() == Action::Always {
        Some(true)
    } else {
        Confirm::new()
            .with_prompt(format!(
                "Create link from `{}` to `{}`?",
                link.display(),
                original.display()
            ))
            .default(true)
            .interact_opt()
            .map_err(|Error::IO(err)| err)?
    };

    let Some(create_link) = create_link else {
        return Ok(ShouldExit::Yes);
    };

    if !create_link {
        return Ok(ShouldExit::No);
    }

    let link_file_name: String = if cli.ask_to_rename_links() {
        Input::new()
            .with_prompt("Link name")
            .with_initial_text(link_file_name.to_string_lossy())
            .interact_text() // For some reason supports utf-8
            .map_err(|Error::IO(err)| err)?
    } else {
        link_file_name.to_string_lossy().into_owned()
    };

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

fn create_dir(location: &Path, name: &Path, cli: &Cli) -> io::Result<CreateDirContinuation> {
    let create = if cli.create_dirs() == Action::Always {
        Some(true)
    } else {
        Confirm::new()
            .with_prompt(format!(
                "Recreate the `{}` directory in {}?",
                name.display(),
                location.display()
            ))
            .default(true)
            .interact_opt()
            .map_err(|Error::IO(err)| err)?
    };

    let Some(create) = create else {
        return Ok(CreateDirContinuation::Exit);
    };

    if !create {
        return Ok(CreateDirContinuation::Continue);
    }

    let dir_name: String = if cli.ask_to_rename_dirs() {
        Input::new()
            .with_prompt("Dir name")
            .with_initial_text(name.to_string_lossy())
            .interact_text()
            .map_err(|Error::IO(err)| err)?
    } else {
        name.to_string_lossy().into_owned()
    };

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
            }
        };

        let file_type = match get_definitive_file_type(&entry) {
            Ok(file_type) => file_type,
            Err(err) => {
                eprintln!("Failed to get entry file type: {err}");
                continue;
            }
        };

        if file_type.is_file() {
            match link_file(&entry.path(), &target.join(entry.file_name()), cli) {
                Ok(ShouldExit::No) => continue,
                Ok(ShouldExit::Yes) => return ShouldExit::Yes,
                Err(err) => {
                    eprintln!("Encountered error while trying to link file: {err}");
                    continue;
                }
            }
        }

        match create_dir(target, Path::new(&entry.file_name()), cli) {
            Ok(CreateDirContinuation::Exit) => return ShouldExit::Yes,
            Ok(CreateDirContinuation::Continue) => continue,
            Ok(CreateDirContinuation::MaybeRecurse(new_dir_path)) => {
                if cli.recurse() == Action::Never {
                    continue;
                }

                let recurse = match cli.recurse() {
                    Action::Never => continue,
                    Action::Always => Some(true),
                    Action::Ask => {
                        let recurse = Confirm::new()
                            .with_prompt("Should we recurse into the recreated folder?")
                            .default(true)
                            .interact_opt()
                            .map_err(|Error::IO(err)| err);
                        if let Err(err) = recurse {
                            eprintln!("Error in prompt: {err}");
                            continue;
                        }

                        recurse.unwrap()
                    }
                };

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
                    }
                };

                if recurse_into_dir(recurse_dirs, &new_dir_path, cli).should_exit() {
                    return ShouldExit::Yes;
                }
            }
            Err(err) => {
                eprintln!("Failed to create file: {err}");
                continue;
            }
        }
    }

    ShouldExit::No
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Some(gen) = cli.completions() {
        generate(gen, &mut Cli::command(), Cli::command().get_name().to_string(), &mut io::stdout());
        return ExitCode::SUCCESS;
    }

    if cli.base.is_file() {
        let base_file_name = cli
            .base
            .file_name()
            .expect("<BASE> was provided a file that doesn't have a valid filename by Rust rules");
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
        }
    };

    recurse_into_dir(dirs, &cli.target, &cli);

    ExitCode::SUCCESS
}
