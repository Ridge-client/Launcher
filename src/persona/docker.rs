//! Docker orchestration for persona containers, volumes and networks via bollard.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, ListContainersOptions, LogsOptions,
    RemoveContainerOptions, StopContainerOptions, UploadToContainerOptions,
};
use bollard::errors::Error as DockerError;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::{BuildImageOptions, CreateImageOptions};
use bollard::models::{
    DeviceMapping, HealthConfig, HealthStatusEnum, HostConfig, Mount, MountTypeEnum, PortBinding,
};
use bollard::network::{CreateNetworkOptions, ListNetworksOptions};
use bollard::volume::{CreateVolumeOptions, ListVolumesOptions, RemoveVolumeOptions};
use bollard::Docker;
use futures_util::StreamExt;

use super::firewall::{self, Protections, FIREWALL_IMAGE, READY_FILE};
use super::vpn::VpnConfig;
use super::{
    normalize_image_ref, PersonaNames, GUI_CONTAINER_PORT, LABEL_MANAGED, LABEL_PERSONA,
    LABEL_PROTECTIONS, LABEL_ROLE,
};
use crate::ui;

const VPN_HEALTH_TIMEOUT: Duration = Duration::from_secs(120);
const FIREWALL_READY_TIMEOUT: Duration = Duration::from_secs(30);
/// The user name of a persona's password-protected remote screen.
pub const GUI_USER: &str = "ridge";

pub struct StartOptions {
    pub names: PersonaNames,
    pub browser_image: String,
    pub vpn_image: String,
    pub vpn: Option<VpnConfig>,
    pub host_port: u16,
    pub tz: String,
    /// Tar archive extracted at `/` in the browser container before it starts.
    pub policy_archive: Option<Vec<u8>>,
    pub pull: bool,
    /// Start script of the persona's firewall container (`firewall::rules_script`); none means
    /// the browser joins its network directly. Ignored behind a VPN, whose own firewall applies.
    pub firewall_script: Option<String>,
    /// Password of the persona's remote screen (user [`GUI_USER`]); none leaves it open on this PC.
    pub gui_password: Option<String>,
    /// The protections asked for, recorded on the browser container as far as they apply.
    pub protections: Protections,
}

/// Summary of one persona for `persona list`.
#[derive(Debug, Default)]
pub struct PersonaSummary {
    pub has_volume: bool,
    pub browser: Option<String>,
    pub vpn: Option<String>,
    /// What its browser was started with; none for a persona started by an older Ridge.
    pub protections: Option<Protections>,
}

/// The test persona of the Settings checks: a network and a firewall, no browser.
const CHECKS_NETWORK: &str = "ridge_checks_net";
const CHECKS_CONTAINER: &str = "ridge_checks_fw";

pub struct Manager {
    docker: Docker,
}

fn is_status(err: &DockerError, code: u16) -> bool {
    matches!(err, DockerError::DockerResponseServerError { status_code, .. } if *status_code == code)
}

/// Treats "not found" (and optionally "not modified") as success.
fn ignore_missing(res: Result<(), DockerError>) -> Result<bool, DockerError> {
    match res {
        Ok(()) => Ok(true),
        Err(e) if is_status(&e, 404) || is_status(&e, 304) => Ok(false),
        Err(e) => Err(e),
    }
}

fn labels(persona: &str, role: &str) -> HashMap<String, String> {
    HashMap::from([
        (LABEL_MANAGED.to_string(), "true".to_string()),
        (LABEL_PERSONA.to_string(), persona.to_string()),
        (LABEL_ROLE.to_string(), role.to_string()),
    ])
}

fn label_filter(pairs: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
    HashMap::from([(
        "label".to_string(),
        pairs.iter().map(|(k, v)| format!("{k}={v}")).collect(),
    )])
}

impl Manager {
    pub async fn connect() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .context("could not connect to Docker — is it installed and running?")?;
        docker
            .ping()
            .await
            .context("cannot reach the Docker daemon — is Docker running and accessible?")?;
        Ok(Self { docker })
    }

    pub async fn start(&self, o: &StartOptions) -> Result<()> {
        let n = &o.names;
        if self.is_running(&n.browser).await? {
            bail!(
                "persona '{}' is already running (run `ridge persona stop {}` first)",
                n.persona,
                n.persona
            );
        }
        // Containers are disposable and recreated so changed flags take effect;
        // everything that must persist lives in the named volume.
        self.remove_containers(n).await?;

        self.ensure_image(&o.browser_image, o.pull).await?;
        if o.vpn.is_some() {
            self.ensure_image(&o.vpn_image, o.pull).await?;
        } else if o.firewall_script.is_some() {
            self.ensure_firewall_image().await?;
        }
        self.ensure_volume(n).await?;
        self.ensure_network(n).await?;

        let gui_port = format!("{GUI_CONTAINER_PORT}/tcp");
        let port_bindings = HashMap::from([(
            gui_port.clone(),
            Some(vec![PortBinding {
                host_ip: Some("127.0.0.1".into()),
                host_port: Some(o.host_port.to_string()),
            }]),
        )]);
        let exposed = HashMap::from([(gui_port, HashMap::new())]);

        let browser_network = match (&o.vpn, &o.firewall_script) {
            (Some(vpn), _) => {
                self.start_vpn(o, vpn, port_bindings.clone(), exposed.clone())
                    .await?;
                // Share the sidecar's network namespace: no route exists except through the tunnel.
                format!("container:{}", n.vpn)
            }
            (None, Some(script)) => {
                self.start_firewall(o, script, port_bindings.clone(), exposed.clone())
                    .await?;
                // Share the firewall's network namespace: its rules apply, and the browser
                // (without NET_ADMIN) cannot change them.
                format!("container:{}", n.firewall)
            }
            (None, None) => n.network.clone(),
        };
        let behind_vpn = o.vpn.is_some() || o.firewall_script.is_some();
        let mut env = vec![
            "PUID=1000".to_string(),
            "PGID=1000".to_string(),
            format!("TZ={}", o.tz),
        ];
        if let Some(password) = &o.gui_password {
            env.push(format!("CUSTOM_USER={GUI_USER}"));
            env.push(format!("PASSWORD={password}"));
        }

        let config = Config {
            image: Some(normalize_image_ref(&o.browser_image)),
            env: Some(env),
            labels: Some({
                // what really applies: behind a VPN its own firewall is used instead of Ridge's
                let applied = Protections {
                    password: o.gui_password.is_some(),
                    ..if o.vpn.is_none() && o.firewall_script.is_some() {
                        o.protections
                    } else {
                        Protections::NONE
                    }
                };
                let mut l = labels(&n.persona, "browser");
                l.insert(LABEL_PROTECTIONS.to_string(), applied.to_label());
                l
            }),
            exposed_ports: (!behind_vpn).then_some(exposed),
            host_config: Some(HostConfig {
                network_mode: Some(browser_network),
                port_bindings: (!behind_vpn).then_some(port_bindings),
                mounts: Some(vec![Mount {
                    target: Some("/config".into()),
                    source: Some(n.volume.clone()),
                    typ: Some(MountTypeEnum::VOLUME),
                    ..Default::default()
                }]),
                // Browsers need a large /dev/shm or tabs crash.
                shm_size: Some(1 << 30),
                security_opt: Some(vec!["no-new-privileges:true".into()]),
                ..Default::default()
            }),
            ..Default::default()
        };
        self.create(&n.browser, config).await?;

        if let Some(archive) = &o.policy_archive {
            self.docker
                .upload_to_container(
                    &n.browser,
                    Some(UploadToContainerOptions {
                        path: "/".to_string(),
                        ..Default::default()
                    }),
                    archive.clone().into(),
                )
                .await
                .context("installing Firefox policies into the browser container")?;
        }

        self.docker
            .start_container::<String>(&n.browser, None)
            .await
            .with_context(|| format!("starting {}", n.browser))?;
        Ok(())
    }

    async fn start_vpn(
        &self,
        o: &StartOptions,
        vpn: &VpnConfig,
        port_bindings: HashMap<String, Option<Vec<PortBinding>>>,
        exposed: HashMap<String, HashMap<(), ()>>,
    ) -> Result<()> {
        let n = &o.names;
        let mut env = vpn.container_env();
        env.push(format!("TZ={}", o.tz));
        let config = Config {
            image: Some(normalize_image_ref(&o.vpn_image)),
            env: Some(env),
            labels: Some(labels(&n.persona, "vpn")),
            exposed_ports: Some(exposed),
            host_config: Some(HostConfig {
                network_mode: Some(n.network.clone()),
                port_bindings: Some(port_bindings),
                cap_add: Some(vec!["NET_ADMIN".into()]),
                devices: Some(vec![DeviceMapping {
                    path_on_host: Some("/dev/net/tun".into()),
                    path_in_container: Some("/dev/net/tun".into()),
                    cgroup_permissions: Some("rwm".into()),
                }]),
                security_opt: Some(vec!["no-new-privileges:true".into()]),
                ..Default::default()
            }),
            ..Default::default()
        };
        self.create(&n.vpn, config).await?;
        self.docker
            .start_container::<String>(&n.vpn, None)
            .await
            .with_context(|| format!("starting {}", n.vpn))?;
        self.wait_healthy(&n.vpn, "VPN tunnel", VPN_HEALTH_TIMEOUT)
            .await
    }

    /// The persona's firewall: it owns the network and the screen's port; healthy once its rules
    /// are in place.
    async fn start_firewall(
        &self,
        o: &StartOptions,
        script: &str,
        port_bindings: HashMap<String, Option<Vec<PortBinding>>>,
        exposed: HashMap<String, HashMap<(), ()>>,
    ) -> Result<()> {
        let n = &o.names;
        let second = 1_000_000_000;
        let config = Config {
            image: Some(FIREWALL_IMAGE.to_string()),
            cmd: Some(vec!["sh".into(), "-c".into(), script.to_string()]),
            labels: Some(labels(&n.persona, "firewall")),
            exposed_ports: Some(exposed),
            healthcheck: Some(HealthConfig {
                test: Some(vec![
                    "CMD".into(),
                    "test".into(),
                    "-f".into(),
                    READY_FILE.into(),
                ]),
                interval: Some(second),
                timeout: Some(second),
                retries: Some(30),
                start_period: Some(0),
                start_interval: None,
            }),
            host_config: Some(HostConfig {
                network_mode: Some(n.network.clone()),
                port_bindings: Some(port_bindings),
                cap_add: Some(vec!["NET_ADMIN".into()]),
                security_opt: Some(vec!["no-new-privileges:true".into()]),
                ..Default::default()
            }),
            ..Default::default()
        };
        self.create(&n.firewall, config).await?;
        self.docker
            .start_container::<String>(&n.firewall, None)
            .await
            .with_context(|| format!("starting {}", n.firewall))?;
        self.wait_healthy(&n.firewall, "firewall", FIREWALL_READY_TIMEOUT)
            .await
    }

    /// Builds the small firewall image on this PC, once.
    async fn ensure_firewall_image(&self) -> Result<()> {
        if self.docker.inspect_image(FIREWALL_IMAGE).await.is_ok() {
            return Ok(());
        }
        let mut stream = self.docker.build_image(
            BuildImageOptions {
                dockerfile: "Dockerfile".to_string(),
                t: FIREWALL_IMAGE.to_string(),
                rm: true,
                ..Default::default()
            },
            None,
            Some(firewall::build_context()?.into()),
        );
        while let Some(event) = stream.next().await {
            let event = event.context("building the firewall image")?;
            if let Some(error) = event.error {
                bail!("building the firewall image: {error}");
            }
        }
        Ok(())
    }

    async fn wait_healthy(&self, container: &str, what: &str, timeout: Duration) -> Result<()> {
        let spinner = ui::spinner(&format!("Waiting for the {what}…"));
        let started = Instant::now();
        loop {
            let info = self
                .docker
                .inspect_container(container, None::<InspectContainerOptions>)
                .await?;
            let state = info.state.unwrap_or_default();
            if state.running != Some(true) {
                spinner.finish_and_clear();
                bail!(
                    "the {what} stopped before it was ready. Last log lines:\n{}",
                    self.tail_logs(container).await
                );
            }
            match state.health.and_then(|h| h.status) {
                Some(HealthStatusEnum::HEALTHY) => break,
                // Image without a healthcheck: nothing more to wait for.
                None | Some(HealthStatusEnum::NONE) | Some(HealthStatusEnum::EMPTY) => break,
                _ => {}
            }
            if started.elapsed() > timeout {
                spinner.finish_and_clear();
                bail!(
                    "the {what} was not ready after {}s. Last log lines:\n{}",
                    timeout.as_secs(),
                    self.tail_logs(container).await
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        spinner.finish_and_clear();
        Ok(())
    }

    async fn tail_logs(&self, container: &str) -> String {
        let mut stream = self.docker.logs(
            container,
            Some(LogsOptions::<String> {
                stdout: true,
                stderr: true,
                tail: "25".into(),
                ..Default::default()
            }),
        );
        let mut out = String::new();
        while let Some(Ok(line)) = stream.next().await {
            out.push_str("  ");
            out.push_str(line.to_string().trim_end());
            out.push('\n');
        }
        out
    }

    async fn create(&self, name: &str, config: Config<String>) -> Result<()> {
        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: name.to_string(),
                    platform: None,
                }),
                config,
            )
            .await
            .with_context(|| format!("creating container {name}"))?;
        Ok(())
    }

    async fn is_running(&self, container: &str) -> Result<bool> {
        match self
            .docker
            .inspect_container(container, None::<InspectContainerOptions>)
            .await
        {
            Ok(info) => Ok(info.state.and_then(|s| s.running).unwrap_or(false)),
            Err(e) if is_status(&e, 404) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn ensure_image(&self, image: &str, force_pull: bool) -> Result<()> {
        let image = normalize_image_ref(image);
        if !force_pull && self.docker.inspect_image(&image).await.is_ok() {
            return Ok(());
        }
        let spinner = ui::spinner(&format!("Pulling {image}"));
        let result = self
            .pull_image(&image, |p| {
                spinner.set_message(format!("Pulling {image}: {:.0}%", p * 100.0))
            })
            .await;
        spinner.finish_and_clear();
        result
    }

    /// Downloads `image` unless it is already present, so a later start is quick.
    /// Reports overall progress (0.0 to 1.0) while downloading.
    pub async fn prepare_image(&self, image: &str, on_progress: impl FnMut(f32)) -> Result<()> {
        let image = normalize_image_ref(image);
        if self.docker.inspect_image(&image).await.is_ok() {
            return Ok(());
        }
        self.pull_image(&image, on_progress).await
    }

    async fn pull_image(&self, image: &str, mut on_progress: impl FnMut(f32)) -> Result<()> {
        // bytes (done, total) per layer; a finished layer counts as fully done
        let mut layers: HashMap<String, (i64, i64)> = HashMap::new();
        let mut reported = 0.0_f32;
        let mut stream = self.docker.create_image(
            Some(CreateImageOptions {
                from_image: image.to_string(),
                ..Default::default()
            }),
            None,
            None,
        );
        while let Some(event) = stream.next().await {
            let event = event.with_context(|| format!("pulling {image}"))?;
            let (Some(id), Some(status)) = (event.id, event.status) else {
                continue;
            };
            match status.as_str() {
                "Downloading" => {
                    if let Some((Some(done), Some(total))) =
                        event.progress_detail.map(|d| (d.current, d.total))
                    {
                        layers.insert(id, (done, total.max(1)));
                    }
                }
                "Download complete" | "Pull complete" | "Already exists" => {
                    if let Some(layer) = layers.get_mut(&id) {
                        layer.0 = layer.1;
                    }
                }
                _ => {}
            }
            let (done, total) = layers
                .values()
                .fold((0, 0), |(d, t), (ld, lt)| (d + ld, t + lt));
            // never move backwards when a new layer is discovered
            let p = (done as f32 / total.max(1) as f32).max(reported);
            if p - reported >= 0.01 {
                reported = p;
                on_progress(p);
            }
        }
        on_progress(1.0);
        Ok(())
    }

    async fn ensure_volume(&self, n: &PersonaNames) -> Result<()> {
        if self.docker.inspect_volume(&n.volume).await.is_ok() {
            return Ok(());
        }
        self.docker
            .create_volume(CreateVolumeOptions {
                name: n.volume.clone(),
                driver: "local".to_string(),
                labels: labels(&n.persona, "data"),
                ..Default::default()
            })
            .await
            .with_context(|| format!("creating volume {}", n.volume))?;
        Ok(())
    }

    async fn ensure_network(&self, n: &PersonaNames) -> Result<()> {
        let existing = self
            .docker
            .list_networks(Some(ListNetworksOptions {
                filters: HashMap::from([("name".to_string(), vec![n.network.clone()])]),
            }))
            .await?;
        if existing
            .iter()
            .any(|net| net.name.as_deref() == Some(&n.network))
        {
            return Ok(());
        }
        self.docker
            .create_network(CreateNetworkOptions {
                name: n.network.clone(),
                driver: "bridge".to_string(),
                check_duplicate: true,
                // Dedicated per-persona bridge with inter-container traffic disabled.
                options: HashMap::from([(
                    "com.docker.network.bridge.enable_icc".to_string(),
                    "false".to_string(),
                )]),
                labels: labels(&n.persona, "network"),
                ..Default::default()
            })
            .await
            .with_context(|| format!("creating network {}", n.network))?;
        Ok(())
    }

    /// Stops the persona's containers; returns how many were actually running.
    pub async fn stop(&self, n: &PersonaNames) -> Result<usize> {
        let mut stopped = 0;
        // Browser first: it lives in the VPN's or firewall's network namespace.
        for name in [&n.browser, &n.firewall, &n.vpn] {
            let res = self
                .docker
                .stop_container(name, Some(StopContainerOptions { t: 10 }))
                .await;
            if ignore_missing(res).with_context(|| format!("stopping {name}"))? {
                stopped += 1;
            }
        }
        Ok(stopped)
    }

    async fn remove_containers(&self, n: &PersonaNames) -> Result<usize> {
        let mut removed = 0;
        for name in [&n.browser, &n.firewall, &n.vpn] {
            if self.remove_container(name).await? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn remove_container(&self, name: &str) -> Result<bool> {
        let res = self
            .docker
            .remove_container(
                name,
                Some(RemoveContainerOptions {
                    force: true,
                    v: false,
                    link: false,
                }),
            )
            .await;
        ignore_missing(res).with_context(|| format!("removing container {name}"))
    }

    async fn remove_network(&self, name: &str) -> Result<bool> {
        let res = self.docker.remove_network(name).await;
        ignore_missing(res).with_context(|| format!("removing network {name}"))
    }

    /// Removes containers and network but keeps the data volume.
    pub async fn clean(&self, n: &PersonaNames) -> Result<usize> {
        let removed = self.remove_containers(n).await?;
        self.remove_network(&n.network).await?;
        Ok(removed)
    }

    /// Removes every Ridge-managed container and network; volumes are untouched.
    pub async fn clean_all(&self) -> Result<usize> {
        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: label_filter(&[(LABEL_MANAGED, "true")]),
                ..Default::default()
            }))
            .await?;
        // Remove browsers before VPN sidecars whose namespace they share.
        let mut ids: Vec<(bool, String)> = containers
            .into_iter()
            .filter_map(|c| {
                // browsers first: they live in their VPN's or firewall's network namespace
                let is_sidecar = c
                    .labels
                    .as_ref()
                    .and_then(|l| l.get(LABEL_ROLE))
                    .is_some_and(|r| r != "browser");
                c.id.map(|id| (is_sidecar, id))
            })
            .collect();
        ids.sort();
        let mut removed = 0;
        for (_, id) in &ids {
            if self.remove_container(id).await? {
                removed += 1;
            }
        }

        let networks = self
            .docker
            .list_networks(Some(ListNetworksOptions {
                filters: label_filter(&[(LABEL_MANAGED, "true")]),
            }))
            .await?;
        for net in networks {
            if let Some(id) = net.id {
                self.remove_network(&id).await?;
            }
        }
        Ok(removed)
    }

    /// Deletes containers, network AND the persistent profile volume.
    pub async fn purge(&self, n: &PersonaNames) -> Result<bool> {
        self.clean(n).await?;
        match self
            .docker
            .remove_volume(&n.volume, Some(RemoveVolumeOptions { force: true }))
            .await
        {
            Ok(()) => Ok(true),
            Err(e) if is_status(&e, 404) => Ok(false),
            Err(e) => Err(e).with_context(|| format!("removing volume {}", n.volume)),
        }
    }

    pub async fn list(&self) -> Result<BTreeMap<String, PersonaSummary>> {
        let mut personas: BTreeMap<String, PersonaSummary> = BTreeMap::new();

        let volumes = self
            .docker
            .list_volumes(Some(ListVolumesOptions {
                filters: label_filter(&[(LABEL_MANAGED, "true"), (LABEL_ROLE, "data")]),
            }))
            .await?;
        for vol in volumes.volumes.unwrap_or_default() {
            if let Some(persona) = vol.labels.get(LABEL_PERSONA) {
                personas.entry(persona.clone()).or_default().has_volume = true;
            }
        }

        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: label_filter(&[(LABEL_MANAGED, "true")]),
                ..Default::default()
            }))
            .await?;
        for c in containers {
            let labels = c.labels.unwrap_or_default();
            let Some(persona) = labels.get(LABEL_PERSONA) else {
                continue;
            };
            let entry = personas.entry(persona.clone()).or_default();
            let status = c.status.or(c.state).unwrap_or_else(|| "unknown".into());
            match labels.get(LABEL_ROLE).map(String::as_str) {
                Some("browser") => {
                    entry.browser = Some(status);
                    entry.protections = labels
                        .get(LABEL_PROTECTIONS)
                        .map(|l| Protections::from_label(l));
                }
                Some("vpn") => entry.vpn = Some(status),
                _ => {}
            }
        }
        Ok(personas)
    }

    /// The persona's address on its own Docker network (its firewall's, or its browser's).
    pub async fn persona_ip(&self, n: &PersonaNames) -> Option<String> {
        for name in [&n.firewall, &n.browser] {
            let Ok(info) = self
                .docker
                .inspect_container(name, None::<InspectContainerOptions>)
                .await
            else {
                continue;
            };
            let ip = info
                .network_settings
                .and_then(|s| s.networks)
                .and_then(|nets| nets.into_values().find_map(|net| net.ip_address))
                .filter(|ip| !ip.is_empty());
            if ip.is_some() {
                return ip;
            }
        }
        None
    }

    /// Runs `checks` in a test persona: its own network behind a firewall with `rules` (a
    /// `firewall::rules_script`), made for this and removed after.
    pub async fn run_in_test_persona(&self, rules: &str, checks: &str) -> Result<String> {
        self.ensure_firewall_image().await?;
        self.remove_test_persona().await?;
        let mut l = HashMap::from([
            (LABEL_MANAGED.to_string(), "true".to_string()),
            (LABEL_ROLE.to_string(), "checks".to_string()),
        ]);
        self.docker
            .create_network(CreateNetworkOptions {
                name: CHECKS_NETWORK.to_string(),
                driver: "bridge".to_string(),
                check_duplicate: true,
                options: HashMap::from([(
                    "com.docker.network.bridge.enable_icc".to_string(),
                    "false".to_string(),
                )]),
                labels: l.clone(),
                ..Default::default()
            })
            .await
            .context("creating the test persona's network")?;
        let second = 1_000_000_000;
        let config = Config {
            image: Some(FIREWALL_IMAGE.to_string()),
            cmd: Some(vec!["sh".into(), "-c".into(), rules.to_string()]),
            labels: Some(std::mem::take(&mut l)),
            healthcheck: Some(HealthConfig {
                test: Some(vec![
                    "CMD".into(),
                    "test".into(),
                    "-f".into(),
                    READY_FILE.into(),
                ]),
                interval: Some(second),
                timeout: Some(second),
                retries: Some(30),
                start_period: Some(0),
                start_interval: None,
            }),
            host_config: Some(HostConfig {
                network_mode: Some(CHECKS_NETWORK.to_string()),
                cap_add: Some(vec!["NET_ADMIN".into()]),
                security_opt: Some(vec!["no-new-privileges:true".into()]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = async {
            self.create(CHECKS_CONTAINER, config).await?;
            self.docker
                .start_container::<String>(CHECKS_CONTAINER, None)
                .await
                .context("starting the test persona")?;
            self.wait_healthy(CHECKS_CONTAINER, "test persona", FIREWALL_READY_TIMEOUT)
                .await?;
            self.exec(CHECKS_CONTAINER, checks).await
        }
        .await;
        self.remove_test_persona().await?;
        result
    }

    async fn remove_test_persona(&self) -> Result<()> {
        self.remove_container(CHECKS_CONTAINER).await?;
        self.remove_network(CHECKS_NETWORK).await?;
        Ok(())
    }

    /// Runs a bash script inside the persona's browser container and returns what it printed.
    pub async fn run_in_browser(&self, n: &PersonaNames, script: &str) -> Result<String> {
        self.exec(&n.browser, script).await
    }

    /// Runs a bash script inside `container` and returns what it printed.
    async fn exec(&self, container: &str, script: &str) -> Result<String> {
        let exec = self
            .docker
            .create_exec(
                container,
                CreateExecOptions {
                    cmd: Some(vec!["bash", "-c", script]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .with_context(|| format!("running checks in {container}"))?;
        let mut out = String::new();
        if let StartExecResults::Attached { mut output, .. } =
            self.docker.start_exec(&exec.id, None).await?
        {
            while let Some(chunk) = output.next().await {
                out.push_str(&chunk?.to_string());
            }
        }
        Ok(out)
    }

    /// Host port currently published for the persona's GUI, if any.
    pub async fn gui_port(&self, n: &PersonaNames) -> Option<u16> {
        let key = format!("{GUI_CONTAINER_PORT}/tcp");
        for name in [&n.firewall, &n.vpn, &n.browser] {
            let Ok(info) = self
                .docker
                .inspect_container(name, None::<InspectContainerOptions>)
                .await
            else {
                continue;
            };
            let port = info
                .host_config
                .and_then(|h| h.port_bindings)
                .and_then(|mut b| b.remove(&key).flatten())
                .and_then(|b| b.into_iter().next())
                .and_then(|b| b.host_port)
                .and_then(|p| p.parse().ok());
            if port.is_some() {
                return port;
            }
        }
        None
    }
}
