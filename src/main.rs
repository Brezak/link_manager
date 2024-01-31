use std::{
    env, fs,
    io::{self, stdin, stdout, Write},
    path::Path,
};

fn read_yes_no() -> io::Result<bool> {
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    Ok(input.to_lowercase() == "y\n")
}

fn handle_file(file: &Path, target_path: &Path) -> io::Result<()> {
    println!(
        "Create link for file {} in {}? y\\N",
        file.display(),
        target_path.display()
    );
    if !read_yes_no()? {
        return Ok(());
    }

    print!("Rename file to (leave blank to keep original name):");
    stdout().flush()?;
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    input = input.strip_suffix('\n').map(String::from).unwrap_or(input);
    if input != "" {
        fs::hard_link(file, target_path.join(input)).and(Ok(()))
    } else {
        fs::hard_link(file, target_path).and(Ok(()))
    }
}

fn walk_files(start_path: &Path, target_path: &Path) -> io::Result<()> {
    let start_metadata = fs::metadata(start_path)?;

    if start_metadata.is_file() {
        return handle_file(start_path, target_path);
    }

    for dir in fs::read_dir(start_path)? {
        let dir = match dir {
            Ok(dir) => dir,
            Err(e) => {
                eprintln!("Failed to open access dir: {e}");
                continue;
            }
        };

        if dir.file_type()?.is_file() {
            handle_file(dbg!(&dir.path()), target_path)?;
            continue;
        }

        continue;
        println!(
            "Create directory for {} in {}?",
            dir.path().display(),
            target_path.display()
        );
        let create_dir = read_yes_no()?;
    }

    Ok(())
}

fn main() {
    let mut args = env::args();

    let _executable = args.next().unwrap();

    let start_path = args.next().unwrap();

    let taget_path = args.next().unwrap();

    let start_path = fs::canonicalize(start_path).expect("Failed to get cannonical start path.");
    let target_path = fs::canonicalize(taget_path).expect("Failed to get cannonical target path.");

    dbg!(&target_path);

    walk_files(&start_path, &target_path).unwrap();
}
