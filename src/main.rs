use anyhow::{anyhow, Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_ecs::{types::DesiredStatus, Client as EcsClient};
use aws_types::region::Region;
use clap::Parser;
use inquire::Select;
use std::process::Command;

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

#[derive(Clone, Debug)]
struct TaskChoice {
    id: String,
    last_status: String,
    container_names: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    if !check_binary("aws") {
        return Err(anyhow!(
            "AWS CLI not found. Please install it before using decs."
        ));
    }

    if !check_binary("session-manager-plugin") {
        eprintln!(
            "Warning: session-manager-plugin not found. Install it for ECS Execute Command support."
        );
    }

    let mut loader = aws_config::from_env().behavior_version(BehaviorVersion::latest());

    if let Some(region) = args.region.clone() {
        loader = loader.region(Region::new(region));
    }

    if let Some(profile) = args.profile.clone() {
        loader = loader.profile_name(profile);
    }

    let shared_config = loader.load().await;

    let ecs = EcsClient::new(&shared_config);

    let cluster = match args.cluster {
        Some(c) => c,
        None => select_cluster(&ecs).await?,
    };

    let service = match args.service {
        Some(s) => s,
        None => select_service(&ecs, &cluster).await?,
    };

    let task = match args.task {
        Some(t) => t,
        None => select_task(&ecs, &cluster, &service).await?,
    };

    let container = match args.container {
        Some(c) => c,
        None => select_container(&ecs, &cluster, &task).await?,
    };

    println!(
        "Connecting to cluster={}, service={}, task={}, container={} ...",
        cluster, service, task, container
    );

    exec_into_container(
        &cluster,
        &task,
        &container,
        &args.command,
        args.profile.as_deref(),
        args.region.as_deref(),
    )?;

    Ok(())
}

async fn select_cluster(ecs: &EcsClient) -> Result<String> {
    let arns = ecs
        .list_clusters()
        .send()
        .await
        .context("failed to list clusters")?
        .cluster_arns
        .unwrap_or_default();

    if arns.is_empty() {
        return Err(anyhow!("No ECS clusters found in this account/region"));
    }

    let names: Vec<String> = arns.iter().map(|s| extract_name(s)).collect();
    let cluster = Select::new("Select Cluster:", names)
        .prompt()
        .context("cluster selection canceled")?;
    Ok(cluster)
}

async fn select_service(ecs: &EcsClient, cluster: &str) -> Result<String> {
    let arns = ecs
        .list_services()
        .cluster(cluster)
        .send()
        .await
        .context("failed to list services")?
        .service_arns
        .unwrap_or_default();

    if arns.is_empty() {
        return Err(anyhow!("No services found in cluster {}", cluster));
    }

    let names: Vec<String> = arns.iter().map(|s| extract_name(s)).collect();
    let service = Select::new("Select Service:", names)
        .prompt()
        .context("service selection canceled")?;
    Ok(service)
}

async fn select_task(ecs: &EcsClient, cluster: &str, service: &str) -> Result<String> {
    let task_arns = ecs
        .list_tasks()
        .cluster(cluster)
        .service_name(service)
        .desired_status(DesiredStatus::Running)
        .send()
        .await
        .context("failed to list tasks")?
        .task_arns
        .unwrap_or_default();

    if task_arns.is_empty() {
        return Err(anyhow!(
            "No RUNNING tasks found for service {} in cluster {}",
            service,
            cluster
        ));
    }

    let described = ecs
        .describe_tasks()
        .cluster(cluster)
        .set_tasks(Some(task_arns))
        .send()
        .await
        .context("failed to describe tasks")?;

    let mut tasks: Vec<TaskChoice> = described
        .tasks
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| {
            let task_arn = t.task_arn?;
            let last_status = t.last_status.unwrap_or_else(|| "UNKNOWN".into());
            let containers = t
                .containers
                .unwrap_or_default()
                .into_iter()
                .filter_map(|c| c.name)
                .collect::<Vec<_>>();
            Some(TaskChoice {
                id: extract_name(&task_arn),
                last_status,
                container_names: containers,
            })
        })
        .collect();

    tasks.sort_by(|a, b| a.id.cmp(&b.id));

    let display: Vec<String> = tasks
        .iter()
        .map(|t| {
            format!(
                "{} ({}), containers: {}",
                t.id,
                t.last_status,
                t.container_names.join(", ")
            )
        })
        .collect();

    let selection = Select::new("Select Task:", display.clone())
        .prompt()
        .context("task selection canceled")?;

    // Map back to id
    let idx = display
        .iter()
        .position(|d| d == &selection)
        .ok_or_else(|| anyhow!("selected task not found"))?;
    Ok(tasks[idx].id.clone())
}

async fn select_container(ecs: &EcsClient, cluster: &str, task_id: &str) -> Result<String> {
    let described = ecs
        .describe_tasks()
        .cluster(cluster)
        .tasks(task_id)
        .send()
        .await
        .context("failed to describe task for containers")?;

    let containers = described
        .tasks
        .unwrap_or_default()
        .into_iter()
        .flat_map(|t| t.containers.unwrap_or_default())
        .filter_map(|c| c.name)
        .collect::<Vec<_>>();

    if containers.is_empty() {
        return Err(anyhow!("No containers found for task {}", task_id));
    }

    let container = if containers.len() == 1 {
        containers[0].clone()
    } else {
        Select::new("Select Container:", containers)
            .prompt()
            .context("container selection canceled")?
    };

    Ok(container)
}

fn exec_into_container(
    cluster: &str,
    task_id: &str,
    container: &str,
    command: &str,
    profile: Option<&str>,
    region: Option<&str>,
) -> Result<()> {
    let task_arg = if task_id.starts_with("arn:") {
        task_id.to_string()
    } else {
        task_id.to_string()
    };

    let mut cmd = Command::new("aws");
    cmd.arg("ecs")
        .arg("execute-command")
        .arg("--cluster")
        .arg(cluster)
        .arg("--task")
        .arg(task_arg)
        .arg("--container")
        .arg(container)
        .arg("--interactive")
        .arg("--command")
        .arg(command);

    if let Some(profile) = profile {
        cmd.env("AWS_PROFILE", profile);
    }
    if let Some(region) = region {
        cmd.env("AWS_REGION", region);
    }

    let status = cmd
        .status()
        .context("failed to start aws cli execute-command")?;

    if !status.success() {
        return Err(anyhow!("aws ecs execute-command exited with {:?}", status));
    }

    Ok(())
}

fn extract_name(arn: &str) -> String {
    arn.split('/').last().unwrap_or(arn).to_string()
}

fn check_binary(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
