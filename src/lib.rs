use std::future::Future;
use std::pin::Pin;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_ecs::{types::DesiredStatus, Client as EcsClient};
use aws_types::region::Region;
use inquire::Select;

/// Parsed arguments passed from CLI.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub cluster: Option<String>,
    pub service: Option<String>,
    pub task: Option<String>,
    pub container: Option<String>,
    pub profile: Option<String>,
    pub region: Option<String>,
    pub command: String,
}

/// Execution plan determined from config + user interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub cluster: String,
    pub service: String,
    pub task_id: String,
    pub container: String,
    pub command: String,
    pub profile: Option<String>,
    pub region: Option<String>,
}

/// Simple task info used for prompting and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInfo {
    pub id: String,
    pub last_status: String,
    pub container_names: Vec<String>,
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstraction over ECS data access to allow mocking in tests.
pub trait EcsApi: Send + Sync {
    fn list_clusters<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>>;
    fn list_services<'a>(&'a self, cluster: &'a str) -> BoxFuture<'a, Result<Vec<String>>>;
    fn list_running_tasks<'a>(
        &'a self,
        cluster: &'a str,
        service: &'a str,
    ) -> BoxFuture<'a, Result<Vec<TaskInfo>>>;
    fn list_containers<'a>(
        &'a self,
        cluster: &'a str,
        task_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>>;
}

/// Abstraction over user prompts to keep business logic testable.
pub trait Prompter: Send + Sync {
    fn select_cluster(&self, clusters: Vec<String>) -> Result<String>;
    fn select_service(&self, services: Vec<String>) -> Result<String>;
    fn select_task(&self, tasks: Vec<TaskInfo>) -> Result<TaskInfo>;
    fn select_container(&self, containers: Vec<String>) -> Result<String>;
}

/// Build the execution plan using provided args, ECS data, and prompts where needed.
pub async fn build_plan(
    cfg: &AppConfig,
    ecs: &dyn EcsApi,
    prompter: &dyn Prompter,
) -> Result<ExecutionPlan> {
    let cluster = match cfg.cluster.clone() {
        Some(c) => c,
        None => {
            let clusters = ecs.list_clusters().await?;
            if clusters.is_empty() {
                return Err(anyhow!("No ECS clusters found"));
            }
            prompter.select_cluster(clusters)?
        }
    };

    let service = match cfg.service.clone() {
        Some(s) => s,
        None => {
            let services = ecs.list_services(&cluster).await?;
            if services.is_empty() {
                return Err(anyhow!("No services found in cluster {}", cluster));
            }
            prompter.select_service(services)?
        }
    };

    let task_info = match cfg.task.clone() {
        Some(t) => TaskInfo {
            id: t,
            last_status: "UNKNOWN".into(),
            container_names: vec![],
        },
        None => {
            let tasks = ecs.list_running_tasks(&cluster, &service).await?;
            if tasks.is_empty() {
                return Err(anyhow!(
                    "No RUNNING tasks found for service {} in cluster {}",
                    service,
                    cluster
                ));
            }
            prompter.select_task(tasks)?
        }
    };

    let container = match cfg.container.clone() {
        Some(c) => c,
        None => {
            let containers = ecs.list_containers(&cluster, &task_info.id).await?;
            if containers.is_empty() {
                return Err(anyhow!("No containers found for task {}", task_info.id));
            }
            prompter.select_container(containers)?
        }
    };

    Ok(ExecutionPlan {
        cluster,
        service,
        task_id: task_info.id,
        container,
        command: cfg.command.clone(),
        profile: cfg.profile.clone(),
        region: cfg.region.clone(),
    })
}

/// Execute an already built plan using AWS CLI (ecs execute-command).
pub fn execute_plan(plan: &ExecutionPlan) -> Result<()> {
    ensure_binary(
        "aws",
        "AWS CLI not found. Please install it before using decs.",
    )?;

    // session-manager-plugin is optional but warned.
    if !has_binary("session-manager-plugin") {
        eprintln!(
            "Warning: session-manager-plugin not found. Install it for ECS Execute Command support."
        );
    }

    let mut cmd = Command::new("aws");
    cmd.arg("ecs")
        .arg("execute-command")
        .arg("--cluster")
        .arg(&plan.cluster)
        .arg("--task")
        .arg(&plan.task_id)
        .arg("--container")
        .arg(&plan.container)
        .arg("--interactive")
        .arg("--command")
        .arg(&plan.command);

    if let Some(profile) = &plan.profile {
        cmd.env("AWS_PROFILE", profile);
    }
    if let Some(region) = &plan.region {
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

/// AWS-backed implementation of `EcsApi`.
pub struct AwsEcsApi {
    client: EcsClient,
}

impl AwsEcsApi {
    pub async fn from_env(profile: Option<String>, region: Option<String>) -> Result<Self> {
        let mut loader = aws_config::from_env().behavior_version(BehaviorVersion::latest());

        if let Some(region) = region {
            loader = loader.region(Region::new(region));
        }
        if let Some(profile) = profile {
            loader = loader.profile_name(profile);
        }

        let shared_config = loader.load().await;
        Ok(Self {
            client: EcsClient::new(&shared_config),
        })
    }
}

impl EcsApi for AwsEcsApi {
    fn list_clusters<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let arns = self
                .client
                .list_clusters()
                .send()
                .await
                .context("failed to list clusters")?
                .cluster_arns
                .unwrap_or_default();
            Ok(arns.into_iter().map(|s| extract_name(&s)).collect())
        })
    }

    fn list_services<'a>(&'a self, cluster: &'a str) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let arns = self
                .client
                .list_services()
                .cluster(cluster)
                .send()
                .await
                .context("failed to list services")?
                .service_arns
                .unwrap_or_default();

            Ok(arns.into_iter().map(|s| extract_name(&s)).collect())
        })
    }

    fn list_running_tasks<'a>(
        &'a self,
        cluster: &'a str,
        service: &'a str,
    ) -> BoxFuture<'a, Result<Vec<TaskInfo>>> {
        Box::pin(async move {
            let task_arns = self
                .client
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
                return Ok(vec![]);
            }

            let described = self
                .client
                .describe_tasks()
                .cluster(cluster)
                .set_tasks(Some(task_arns))
                .send()
                .await
                .context("failed to describe tasks")?;

            let tasks = described
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
                    Some(TaskInfo {
                        id: extract_name(&task_arn),
                        last_status,
                        container_names: containers,
                    })
                })
                .collect();

            Ok(tasks)
        })
    }

    fn list_containers<'a>(
        &'a self,
        cluster: &'a str,
        task_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let described = self
                .client
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

            Ok(containers)
        })
    }
}

/// `Prompter` implementation backed by `inquire` for interactive selection.
pub struct InquirePrompter;

impl Prompter for InquirePrompter {
    fn select_cluster(&self, clusters: Vec<String>) -> Result<String> {
        Select::new("Select Cluster:", clusters)
            .prompt()
            .context("cluster selection canceled")
    }

    fn select_service(&self, services: Vec<String>) -> Result<String> {
        Select::new("Select Service:", services)
            .prompt()
            .context("service selection canceled")
    }

    fn select_task(&self, tasks: Vec<TaskInfo>) -> Result<TaskInfo> {
        let labels: Vec<String> = tasks
            .iter()
            .map(|t| {
                format!(
                    "{} ({}) containers: {}",
                    t.id,
                    t.last_status,
                    t.container_names.join(", ")
                )
            })
            .collect();
        let chosen = Select::new("Select Task:", labels.clone())
            .prompt()
            .context("task selection canceled")?;
        let idx = labels
            .iter()
            .position(|l| l == &chosen)
            .ok_or_else(|| anyhow!("selected task not found"))?;
        Ok(tasks[idx].clone())
    }

    fn select_container(&self, containers: Vec<String>) -> Result<String> {
        Select::new("Select Container:", containers)
            .prompt()
            .context("container selection canceled")
    }
}

fn has_binary(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ensure_binary(name: &str, err: &'static str) -> Result<()> {
    if has_binary(name) {
        Ok(())
    } else {
        Err(anyhow!(err))
    }
}

fn extract_name(arn: &str) -> String {
    arn.split('/').last().unwrap_or(arn).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeEcs {
        clusters: Vec<String>,
        services: Vec<String>,
        tasks: Vec<TaskInfo>,
        containers: Vec<String>,
    }

    impl EcsApi for FakeEcs {
        fn list_clusters<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move { Ok(self.clusters.clone()) })
        }
        fn list_services<'a>(&'a self, _cluster: &'a str) -> BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move { Ok(self.services.clone()) })
        }
        fn list_running_tasks<'a>(
            &'a self,
            _cluster: &'a str,
            _service: &'a str,
        ) -> BoxFuture<'a, Result<Vec<TaskInfo>>> {
            Box::pin(async move { Ok(self.tasks.clone()) })
        }
        fn list_containers<'a>(
            &'a self,
            _cluster: &'a str,
            _task_id: &'a str,
        ) -> BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move { Ok(self.containers.clone()) })
        }
    }

    #[derive(Default)]
    struct FakePrompter {
        next_cluster: String,
        next_service: String,
        next_task_idx: usize,
        next_container: String,
    }

    impl Prompter for FakePrompter {
        fn select_cluster(&self, _clusters: Vec<String>) -> Result<String> {
            Ok(self.next_cluster.clone())
        }
        fn select_service(&self, _services: Vec<String>) -> Result<String> {
            Ok(self.next_service.clone())
        }
        fn select_task(&self, tasks: Vec<TaskInfo>) -> Result<TaskInfo> {
            tasks
                .get(self.next_task_idx)
                .cloned()
                .ok_or_else(|| anyhow!("no task at index"))
        }
        fn select_container(&self, _containers: Vec<String>) -> Result<String> {
            Ok(self.next_container.clone())
        }
    }

    #[tokio::test]
    async fn builds_plan_with_prompts() {
        let ecs = FakeEcs {
            clusters: vec!["prod".into(), "stg".into()],
            services: vec!["api".into()],
            tasks: vec![TaskInfo {
                id: "task123".into(),
                last_status: "RUNNING".into(),
                container_names: vec!["app".into(), "sidecar".into()],
            }],
            containers: vec!["app".into(), "sidecar".into()],
        };
        let prompt = FakePrompter {
            next_cluster: "prod".into(),
            next_service: "api".into(),
            next_task_idx: 0,
            next_container: "app".into(),
        };

        let cfg = AppConfig {
            cluster: None,
            service: None,
            task: None,
            container: None,
            profile: Some("p".into()),
            region: Some("us-east-1".into()),
            command: "/bin/sh".into(),
        };

        let plan = build_plan(&cfg, &ecs, &prompt).await.unwrap();

        assert_eq!(
            plan,
            ExecutionPlan {
                cluster: "prod".into(),
                service: "api".into(),
                task_id: "task123".into(),
                container: "app".into(),
                command: "/bin/sh".into(),
                profile: Some("p".into()),
                region: Some("us-east-1".into()),
            }
        );
    }

    #[tokio::test]
    async fn uses_args_when_provided() {
        let ecs = FakeEcs::default();
        let prompt = FakePrompter::default();
        let cfg = AppConfig {
            cluster: Some("c".into()),
            service: Some("s".into()),
            task: Some("t".into()),
            container: Some("ctr".into()),
            profile: None,
            region: None,
            command: "whoami".into(),
        };

        let plan = build_plan(&cfg, &ecs, &prompt).await.unwrap();

        assert_eq!(
            plan,
            ExecutionPlan {
                cluster: "c".into(),
                service: "s".into(),
                task_id: "t".into(),
                container: "ctr".into(),
                command: "whoami".into(),
                profile: None,
                region: None,
            }
        );
    }

    #[tokio::test]
    async fn errors_when_no_running_tasks() {
        let ecs = FakeEcs {
            clusters: vec!["c".into()],
            services: vec!["s".into()],
            tasks: vec![],
            containers: vec![],
        };
        let prompt = FakePrompter {
            next_cluster: "c".into(),
            next_service: "s".into(),
            ..Default::default()
        };
        let cfg = AppConfig {
            cluster: None,
            service: None,
            task: None,
            container: None,
            profile: None,
            region: None,
            command: "/bin/sh".into(),
        };

        let err = build_plan(&cfg, &ecs, &prompt).await.unwrap_err();
        assert!(err
            .to_string()
            .contains("No RUNNING tasks found for service s in cluster c"));
    }
}
