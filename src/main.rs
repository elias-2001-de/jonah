use anyhow::anyhow;
use clap::{Args, Parser, Subcommand};
use parse_config::{Collection, Project, ProjectInfo};
use std::fs;
use std::path::Path;
use std::process::Command;

mod parse_config;

#[derive(Parser)]
#[command(name = "jonah")]
#[command(
    about = "A ClI build tool that uses Docker",
    long_about = "inside the whale you find the treasure"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Build a single project
    Project(ProjectArgs),
    /// Build a collection of projects
    Collection(ProjectArgs),
    /// removes all the files and containers
    Clean(CleanArgs),
}
#[derive(Args)]
struct CleanArgs {
    /// Temporary directory used during the build process (default: /tmp/jonah)
    #[arg(long, default_value = "/tmp/jonah")]
    temp_dir: String,

    /// Docker image name (default: jonah-build-image)
    #[arg(long, default_value = "jonah-build-image")]
    image_name: String,
}

#[derive(Args)]
struct ProjectArgs {
    /// Path to the TOML file
    file: String,

    /// Path wher the result is stored
    out_path: String,

    /// Docker image name (default: jonah-build-image)
    #[arg(long, default_value = "jonah-build-image")]
    image_name: String,
    /// Docker container name (default: jonah-build-container)
    #[arg(long, default_value = "jonah-build-container")]
    container_name: String,
    /// Temporary directory used during the build process (default: /tmp/jonah)
    #[arg(long, default_value = "/tmp/jonah")]
    temp_dir: String,

    #[arg(long, default_value = "true")]
    print_cmd: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Project(args) => {
            extract_container(
                args.file.to_owned(),
                args.out_path.to_owned(),
                args.container_name.to_owned(),
                args.image_name.to_owned(),
                args.print_cmd,
            )?;
        }
        Commands::Collection(args) => {
            run_collection(
                args.file.to_owned(),
                args.out_path.to_owned(),
                args.temp_dir.to_owned(),
                args.container_name.to_owned(),
                args.image_name.to_owned(),
                args.print_cmd,
            )?;
        }
        Commands::Clean(args) => fs::remove_dir_all(args.temp_dir.to_owned())?,
    }

    return Ok(());
}

fn run_collection(
    config_path: String,
    out_path: String,
    temp_dir: String,
    container_name: String,
    image_name: String,
    print_cmd: bool,
) -> anyhow::Result<()> {
    let toml_str = fs::read_to_string(config_path)?;
    let config: Collection = toml::from_str(&toml_str)?;

    config.validate()?;
    fs::create_dir_all(&out_path)?;
    fs::create_dir_all(&temp_dir)?;

    for project in &config.projects {
        let (build_file, out_path_internal) = match project {
            ProjectInfo::GitRel {
                git_rel,
                build_file,
                out_path,
            } => {
                let Some(base) = &config.git_base else {
                    unreachable!("due to validate");
                };

                let url = match (base.ends_with("/"), git_rel.starts_with("/")) {
                    (true, true) => {
                        let mut base = base.chars().collect::<Vec<_>>();
                        assert_eq!(base.pop(), Some('/'));
                        let base = base.iter().collect::<String>();
                        format!("{base}{git_rel}")
                    }
                    (false, true) => format!("{base}{git_rel}"),
                    (true, false) => format!("{base}{git_rel}"),
                    (false, false) => format!("{base}/{git_rel}"),
                };

                let dir = get_git(&url, &temp_dir, print_cmd)?;

                (format!("{dir}/{build_file}"), out_path.to_owned())
            }
            ProjectInfo::GitUrl {
                git_url,
                build_file,
                out_path,
            } => {
                let dir = get_git(git_url, &temp_dir, print_cmd)?;

                (format!("{dir}/{build_file}"), out_path.to_owned())
            }
            ProjectInfo::LocalPath {
                build_file,
                out_path,
            } => (build_file.to_owned(), out_path.to_owned()),
        };

        extract_container(
            build_file,
            format!("{out_path}/{out_path_internal}"),
            container_name.clone(),
            image_name.clone(),
            print_cmd,
        )?;
    }

    return Ok(());
}

fn get_git(git_url: &String, temp_dir: &String, print_cmd: bool) -> anyhow::Result<String> {
    let temp = git_url
        .split("/")
        .map(|x| x.to_string())
        .collect::<Vec<_>>();
    let mut temp = temp.iter().rev();

    let Some(mut name) = temp.next() else {
        return Err(anyhow!("{git_url} is not a valid git_url"));
    };

    if name.is_empty() {
        match temp.next() {
            Some(val) => name = val,
            None => return Err(anyhow!("{git_url} is not a valid git_url")),
        }
    }

    let mut path = temp_dir.to_string();
    path.push('/');
    if name.ends_with(".git") {
        let mut temp = name.chars().rev();
        assert_eq!(temp.next(), Some('t'));
        assert_eq!(temp.next(), Some('i'));
        assert_eq!(temp.next(), Some('g'));
        assert_eq!(temp.next(), Some('.'));

        let temp = temp.rev().collect::<Vec<_>>();
        assert!(!temp.is_empty());
        path.extend(temp);
    } else {
        path.extend(name.chars());
    }

    let p = Path::new(&path);
    if p.exists() && p.is_dir() {
        // git fetch
        let mut cmd = Command::new("git");
        cmd.args(["fetch", "--depth", "1", "origin"])
            .current_dir(&path);
        if print_cmd {
            println!("🏃 {:?}", cmd);
        }
        cmd.status()?;

        // git reset
        let mut cmd = Command::new("git");
        cmd.args(["reset", "--hard", "origin/HEAD"])
            .current_dir(&path);
        if print_cmd {
            println!("🏃 {:?}", cmd);
        }
        cmd.status()?;
    } else {
        // git clone
        let mut cmd = Command::new("git");
        cmd.args(["clone", &git_url, &path, "--depth", "1"]);
        if print_cmd {
            println!("🏃 {:?}", cmd);
        }
        cmd.status()?;
    }

    return Ok(path);
}

fn extract_container(
    config_file: String,
    out_path: String,
    container_name: String,
    image_name: String,
    print_cmd: bool,
) -> anyhow::Result<()> {
    let toml_str = fs::read_to_string(&config_file)?;
    let config: Project = toml::from_str(&toml_str)?;

    fs::create_dir_all(&out_path)?;
    for host_dir in config.create_host_dirs {
        fs::create_dir_all(&host_dir)?;
    }

    let mut path = config_file.split("/").collect::<Vec<_>>();
    path.pop();

    let path = std::env::current_dir()?.join(path.join("/"));

    // 1. Build the Docker image
    println!("🛠️  Building Docker image...");
    let mut cmd = Command::new("docker");
    cmd.args(["build", "-t", &image_name, "-f", &config.docker, "."])
        .current_dir(path);
    if print_cmd {
        println!("🏃 {:?}", cmd);
    }
    let status = cmd.status()?;

    if !status.success() {
        eprintln!("❌ Docker build failed!");
        return Err(anyhow::anyhow!("Docker build failed"));
    }

    // 2. Run the container
    println!("🚀 Running Docker container...");
    let mut cmd = Command::new("docker");

    cmd.args(["create", &image_name]); // "--name", &container_name,
    if print_cmd {
        println!("🏃 {:?}", cmd);
    }
    let status = cmd.status()?;
    if !status.success() {
        eprintln!("❌ Failed to start the container!");
        return Err(anyhow::anyhow!("Failed to start the container"));
    }

    let docker_id = String::from_utf8(cmd.output()?.stdout)?;
    let docker_id = docker_id.trim();

    // Ensure the output directory exists
    let output_dir = Path::new(&out_path);
    if !output_dir.exists() {
        fs::create_dir(output_dir)?;
    }

    // 3. Extract files from the container
    for export in &config.exports {
        let destination = format!("{out_path}/{}", export.name);
        println!("📦 Extracting {} -> {}", export.path, destination);

        let mut cmd = Command::new("docker");

        cmd.args([
            "cp",
            &format!("{}:{}", docker_id, export.path),
            &destination,
        ]);
        if print_cmd {
            println!("🏃 {:?}", cmd);
        }
        let status = cmd.status()?;
        if !status.success() {
            eprintln!("❌ Failed to copy {}", export.path);
        }
    }

    // 4. Cleanup: Stop and remove the container
    println!("🧹 Cleaning up...");

    let mut cmd = Command::new("docker");
    cmd.args(["rm", &container_name]);
    if print_cmd {
        println!("🏃 {:?}", cmd);
    }
    cmd.output().ok();

    println!("✅ Build and extraction complete!");
    Ok(())
}
