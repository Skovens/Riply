use clap::Args;

/// Interactive setup for MakeMKV installation
#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Show what would be done without executing
    #[arg(long)]
    pub dry_run: bool,
}

/// Run the interactive MakeMKV setup
pub fn run_setup(args: SetupArgs) -> Result<(), anyhow::Error> {
    infra::install_makemkv(args.dry_run)?;
    Ok(())
}
