//! CLI for sigmaos-dkms.

use clap::{Parser, Subcommand, ValueEnum};
use sigmaos_dkms::{current_kernel, default_db_path, DkmsDb, ModuleStatus};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sigmaos-dkms", about = "SigmaOS DKMS kernel module manager")]
struct Cli {
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, ValueEnum)]
enum StatusArg {
    None,
    Built,
    Installed,
    Failed,
}

impl From<StatusArg> for ModuleStatus {
    fn from(s: StatusArg) -> Self {
        match s {
            StatusArg::None => ModuleStatus::None,
            StatusArg::Built => ModuleStatus::Built,
            StatusArg::Installed => ModuleStatus::Installed,
            StatusArg::Failed => ModuleStatus::Failed,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// List managed modules.
    List,
    /// Add a module (name + version).
    Add { name: String, version: String },
    /// Remove a module.
    Remove { name: String },
    /// Set build status for current (or given) kernel.
    Status {
        name: String,
        status: StatusArg,
        #[arg(long)]
        kernel: Option<String>,
    },
    /// List modules missing builds for the current kernel.
    Missing,
    /// Print state path.
    Path,
}

fn db_path(db: &Option<PathBuf>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    db.clone()
        .or_else(default_db_path)
        .ok_or_else(|| "cannot determine db path".into())
}

fn load(db: &Option<PathBuf>) -> Result<DkmsDb, Box<dyn std::error::Error>> {
    let p = db_path(db)?;
    if p.exists() {
        Ok(DkmsDb::load(&p)?)
    } else {
        let d = DkmsDb::default();
        d.save(&p)?;
        Ok(d)
    }
}

fn save(db: &DkmsDb, path: &Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    db.save(&db_path(path)?)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let cli = Cli::parse();
    let mut db = load(&cli.db)?;
    match cli.command {
        Commands::List => {
            for m in db.list() {
                println!("{} {} ({} kernels tracked)", m.name, m.version, m.kernels.len());
            }
        }
        Commands::Add { name, version } => {
            db.add_module(&name, &version)?;
            save(&db, &cli.db)?;
            println!("added {name} {version}");
        }
        Commands::Remove { name } => {
            db.remove_module(&name)?;
            save(&db, &cli.db)?;
            println!("removed {name}");
        }
        Commands::Status { name, status, kernel } => {
            let k = kernel.unwrap_or_else(current_kernel);
            let st: ModuleStatus = status.into();
            db.set_status(&name, &k, st)?;
            save(&db, &cli.db)?;
            println!("{name} on {k}: {st:?}");
        }
        Commands::Missing => {
            let k = current_kernel();
            for m in db.missing_for_kernel(&k) {
                println!("{} {}", m.name, m.version);
            }
        }
        Commands::Path => println!("{}", db_path(&cli.db)?.display()),
    }
    Ok(())
}
