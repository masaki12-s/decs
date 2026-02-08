use anyhow::Result;
use clap::Parser;
use decs::{build_plan, execute_plan, AppConfig, AwsEcsApi, InquirePrompter};

/// decs (Dive into ECS): interactively choose ECS resources and exec into a container.
#[derive(Parser, Debug)]
#[command(author = "decs", version, about = "ECS exec helper", long_about = None)]
struct Cli {
    /// Cluster name (skip prompt when provided)
    #[arg(short, long)]
    cluster: Option<String>,

    /// Service name (skip prompt when provided)
    #[arg(short, long)]
    service: Option<String>,

    /// Task ID (skip prompt when provided)
    #[arg(short, long)]
    task: Option<String>,

    /// Container name (skip prompt when provided)
    #[arg(short = 'n', long)]
    container: Option<String>,

    /// AWS profile to use for requests
    #[arg(long)]
    profile: Option<String>,

    /// AWS region override
    #[arg(long)]
    region: Option<String>,

    /// Command to run inside the container (defaults to /bin/sh)
    #[arg(short = 'x', long, default_value = "/bin/sh")]
    command: String,
}

impl From<Cli> for AppConfig {
    fn from(c: Cli) -> Self {
        AppConfig {
            cluster: c.cluster,
            service: c.service,
            task: c.task,
            container: c.container,
            profile: c.profile,
            region: c.region,
            command: c.command,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg: AppConfig = cli.into();

    let ecs = AwsEcsApi::from_env(cfg.profile.clone(), cfg.region.clone()).await?;
    let prompter = InquirePrompter;

    let plan = build_plan(&cfg, &ecs, &prompter).await?;
    println!(
        "Connecting to cluster={}, service={}, task={}, container={} ...",
        plan.cluster, plan.service, plan.task_id, plan.container
    );
    execute_plan(&plan)?;

    Ok(())
}
