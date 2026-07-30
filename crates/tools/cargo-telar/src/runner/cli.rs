use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "cargo-telar",
    bin_name = "cargo telar",
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<TelarCommand>,
}

#[derive(Subcommand)]
pub(crate) enum TelarCommand {
    /// Start the app with hot reload (default)
    Dev(DevArgs),
    /// Show all component previews with hot reload
    Preview(PreviewArgs),
    /// Build the app for distribution
    Build(BuildArgs),
    /// Render every preview component headlessly and report failures
    Test(TestArgs),
    /// Create a new RSX project (not yet implemented)
    New {
        /// Project name
        name: String,
    },
    /// Check the development environment
    Doctor,
}

#[derive(clap::Args)]
pub(crate) struct CommonArgs {
    /// Package to use
    #[arg(short = 'p', long)]
    pub(crate) package: Option<String>,
    /// Additional Cargo features
    #[arg(short = 'F', long, value_name = "FEATURES")]
    pub(crate) features: Option<String>,
    /// Target platform
    #[arg(long, value_enum, default_value = "desktop")]
    pub(crate) target: Target,
    /// Renderer backend
    #[arg(long, value_enum)]
    pub(crate) backend: Option<BackendArg>,
    /// Extra args passed directly to cargo (after --)
    #[arg(last = true)]
    pub(crate) cargo_args: Vec<String>,
}

/// Flags shared by the hot-reload commands (`dev` and `preview`).
#[derive(clap::Args)]
pub(crate) struct HotArgs {
    #[command(flatten)]
    pub(crate) common: CommonArgs,
    /// Build in release mode
    #[arg(long)]
    pub(crate) release: bool,
    /// Disable hot reload, restart process on changes instead
    #[arg(long)]
    pub(crate) no_hot_reload: bool,
}

#[derive(clap::Args)]
pub(crate) struct DevArgs {
    #[command(flatten)]
    pub(crate) hot: HotArgs,
    /// Devtools overlay
    #[arg(long, value_enum)]
    pub(crate) devtools: Option<DevtoolsArg>,
}

#[derive(clap::Args)]
pub(crate) struct PreviewArgs {
    #[command(flatten)]
    pub(crate) hot: HotArgs,
    /// Preview a specific component by name
    #[arg(long, conflicts_with = "list")]
    pub(crate) component: Option<String>,
    /// List all available previews and exit
    #[arg(long)]
    pub(crate) list: bool,
}

#[derive(clap::Args)]
pub(crate) struct TestArgs {
    #[command(flatten)]
    pub(crate) common: CommonArgs,
    /// Build in release mode
    #[arg(long)]
    pub(crate) release: bool,
}

#[derive(clap::Args)]
pub(crate) struct BuildArgs {
    #[command(flatten)]
    pub(crate) common: CommonArgs,
    /// Output package format
    #[arg(long, value_name = "FORMAT")]
    pub(crate) format: Option<BuildFormat>,
}

#[derive(Clone, ValueEnum)]
pub(crate) enum Target {
    Desktop,
    Android,
}

#[derive(Clone, ValueEnum)]
pub(crate) enum BackendArg {
    Auto,
    Hardware,
    Software,
}

#[derive(Clone, ValueEnum)]
pub(crate) enum DevtoolsArg {
    On,
    Off,
}

#[derive(Clone, ValueEnum)]
pub(crate) enum BuildFormat {
    Appimage,
    Deb,
    Dmg,
    Nsis,
    Apk,
    Dir,
}
