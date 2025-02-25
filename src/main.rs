use anyhow::anyhow;
use clap::{Args, Parser, Subcommand};
use parse_config::{Collection, Project, ProjectInfo};
use std::fs;
use std::path::Path;
use std::process::Command;

mod parse_config;
//   mod url;

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
    Clean(CleanArgs),
}
#[derive(Args)]
struct CleanArgs {
    /// Temporary directory used during the build process (default: /tmp/jonah)
    #[arg(long, default_value = "/tmp/jonah")]
    temp_dir: String,
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
}

fn main() -> anyhow::Result<()> {
    //    fs::create_dir_all(TEMP_DIR)?;

    let cli = Cli::parse();

    match &cli.command {
        Commands::Project(args) => {
            let toml_str = fs::read_to_string(args.file.to_owned())?;
            let config: Project = toml::from_str(&toml_str)?;
            extract_container(
                config,
                args.out_path.to_owned(),
                args.container_name.to_owned(),
                args.image_name.to_owned(),
            )?;
        }
        Commands::Collection(args) => {
            fs::create_dir_all(&args.temp_dir)?;

            let toml_str = fs::read_to_string(args.file.to_owned())?;
            let config: Collection = toml::from_str(&toml_str)?;
            println!("{config:?}");

            run_collection(
                config,
                args.out_path.to_owned(),
                args.temp_dir.to_owned(),
                args.container_name.to_owned(),
                args.image_name.to_owned(),
            )?;
        }
        Commands::Clean(args) => fs::remove_dir_all(args.temp_dir.to_owned())?,
    }

    return Ok(());
}

fn run_collection(
    config: Collection,
    out_path: String,
    temp_dir: String,
    container_name: String,
    image_name: String,
) -> anyhow::Result<()> {
    config.validate()?;

    let mut git_urls = Vec::new();
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

                let url = format!("{base}/{git_rel}");

                let dir = get_git(&url, !git_urls.contains(&url), &temp_dir)?;
                if !git_urls.contains(&url) {
                    git_urls.push(url);
                }

                (format!("{dir}/{build_file}"), out_path.to_owned())
            }
            ProjectInfo::GitUrl {
                git_url,
                build_file,
                out_path,
            } => {
                let dir = get_git(git_url, !git_urls.contains(git_url), &temp_dir)?;
                if !git_urls.contains(git_url) {
                    git_urls.push(git_url.to_owned())
                }

                (format!("{dir}/{build_file}"), out_path.to_owned())
            }
            ProjectInfo::LocalPath {
                build_file,
                out_path,
            } => (build_file.to_owned(), out_path.to_owned()),
        };

        let toml_str = fs::read_to_string(build_file)?;
        let config: Project = toml::from_str(&toml_str)?;
        extract_container(
            config,
            format!("{out_path}/{out_path_internal}"),
            container_name.clone(),
            image_name.clone(),
        )?;
    }

    return Ok(());
}

fn get_git(git_url: &String, run_cmd: bool, temp_dir: &String) -> anyhow::Result<String> {
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

    println!("git clone {git_url} {path}");

    if run_cmd {
        Command::new("git")
            .args(["clone", &git_url, &path, "--depth", "1"])
            .status()?;
    }

    return Ok(path);
}

fn extract_container(
    config: Project,
    out_path: String,
    container_name: String,
    image_name: String,
) -> anyhow::Result<()> {
    // 1. Build the Docker image
    println!("🛠️  Building Docker image...");
    let status = Command::new("docker")
        .args(["build", "-t", &image_name, "-f", &config.docker, "."])
        .status()?;
    if !status.success() {
        eprintln!("❌ Docker build failed!");
        return Err(anyhow::anyhow!("Docker build failed"));
    }

    // 2. Run the container
    println!("🚀 Running Docker container...");
    let status = Command::new("docker")
        .args(["create", "--name", &container_name, &image_name])
        .status()?;
    if !status.success() {
        eprintln!("❌ Failed to start the container!");
        return Err(anyhow::anyhow!("Failed to start the container"));
    }

    // Ensure the output directory exists
    let output_dir = Path::new(&out_path);
    if !output_dir.exists() {
        fs::create_dir(output_dir)?;
    }

    // 3. Extract files from the container
    for export in &config.exports {
        let destination = format!("{out_path}/{}", export.name);
        println!("📦 Extracting {} -> {}", export.path, destination);

        let status = Command::new("docker")
            .args([
                "cp",
                &format!("{}:{}", container_name, export.path),
                &destination,
            ])
            .status()?;
        if !status.success() {
            eprintln!("❌ Failed to copy {}", export.path);
        }
    }

    // 4. Cleanup: Stop and remove the container
    println!("🧹 Cleaning up...");
    // Command::new("docker")
    //    .args(["stop", &container_name])
    //    .output()
    //    .ok();
    Command::new("docker")
        .args(["rm", &container_name])
        .output()
        .ok();

    println!("✅ Build and extraction complete!");
    Ok(())
}
