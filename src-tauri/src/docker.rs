//! Docker discovery for container-hosted databases: list running containers,
//! read their published ports and pick up the credentials given to the image
//! through environment variables, so the user does not have to remember them.

use crate::model::{DockerContainer, DockerPick, DockerStatus};
use std::collections::HashMap;
use tokio::process::Command;

const PORT_HINTS: [(&str, &str, u16); 4] = [
    ("mysql", "mysql", 3306),
    ("mariadb", "mysql", 3306),
    ("postgres", "postgres", 5432),
    ("pgsql", "postgres", 5432),
];

#[cfg(windows)]
fn configure(cmd: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure(_cmd: &mut Command) {}

async fn docker(args: &[&str]) -> Result<std::process::Output, String> {
    let mut cmd = Command::new("docker");
    cmd.args(args);
    configure(&mut cmd);
    tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output())
        .await
        .map_err(|_| "docker 命令超时".to_string())?
        .map_err(|e| format!("无法执行 docker：{e}"))
}

pub async fn inspect() -> DockerStatus {
    match docker(&["version", "--format", "{{.Server.Version}}"]).await {
        Err(e) => {
            return DockerStatus {
                installed: false,
                reachable: false,
                version: None,
                error: Some(e),
                containers: Vec::new(),
            }
        }
        Ok(out) => {
            if !out.status.success() {
                return DockerStatus {
                    installed: true,
                    reachable: false,
                    version: None,
                    error: Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
                    containers: Vec::new(),
                };
            }
            let version = Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
            let containers = match list().await {
                Ok(c) => c,
                Err(e) => {
                    return DockerStatus { installed: true, reachable: true, version, error: Some(e), containers: Vec::new() }
                }
            };
            DockerStatus { installed: true, reachable: true, version, error: None, containers }
        }
    }
}

async fn list() -> Result<Vec<DockerContainer>, String> {
    let out = docker(&[
        "ps",
        "--format",
        "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}",
    ])
    .await?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut rows: Vec<(String, DockerContainer)> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 5 {
            continue;
        }
        let mut picks = parse_ports(f[4]);
        for p in picks.iter_mut() {
            p.driver = driver_of(f[2]);
            if p.host.trim().is_empty() {
                p.host = "127.0.0.1".to_string();
            }
        }
        rows.push((
            f[0].to_string(),
            DockerContainer {
                id: f[0].to_string(),
                name: f[1].to_string(),
                image: f[2].to_string(),
                status: f[3].to_string(),
                picks,
            },
        ));
    }
    fill_credentials(&mut rows).await;
    Ok(rows.into_iter().map(|(_, c)| c).collect())
}

fn driver_of(image: &str) -> Option<String> {
    let low = image.to_lowercase();
    PORT_HINTS.iter().find(|(needle, _, _)| low.contains(needle)).map(|(_, driver, _)| (*driver).to_string())
}

/// `0.0.0.0:3306->3306/tcp, [::]:3306->3306/tcp, 33060/tcp`
fn parse_ports(field: &str) -> Vec<DockerPick> {
    let mut out: Vec<DockerPick> = Vec::new();
    for part in field.split(", ") {
        let Some((left, right)) = part.split_once("->") else { continue };
        let container_port = right
            .split('/')
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(0);
        if container_port == 0 {
            continue;
        }
        let host_port = left.rsplit(':').next().and_then(|p| p.parse::<u16>().ok());
        let Some(host_port) = host_port else { continue };
        let host = if left.starts_with('[') {
            "127.0.0.1".to_string()
        } else {
            match left.split_once(':') {
                Some((h, _)) if h != "0.0.0.0" => h.to_string(),
                _ => "127.0.0.1".to_string(),
            }
        };
        if out.iter().any(|p| p.host_port == host_port) {
            continue;
        }
        out.push(DockerPick {
            container_port,
            host,
            host_port,
            driver: None,
            user: None,
            password: None,
            database: None,
        });
    }
    out
}

/// One inspect call per container; running database containers are few.
async fn fill_credentials(rows: &mut [(String, DockerContainer)]) {
    for (_, c) in rows.iter_mut() {
        let Ok(out) = docker(&["inspect", "--format", "{{range .Config.Env}}{{println .}}{{end}}", &c.id]).await else {
            continue;
        };
        if !out.status.success() {
            continue;
        }
        let mut map: HashMap<String, String> = HashMap::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some((k, v)) = line.split_once('=') {
                map.insert(k.to_string(), v.to_string());
            }
        }
        let get = |keys: &[&str]| keys.iter().find_map(|k| map.get(*k).cloned());
        let (user, pass, db) = match driver_of(&c.image).as_deref() {
            Some("mysql") => match get(&["MYSQL_USER"]) {
                Some(u) => (Some(u), get(&["MYSQL_PASSWORD"]), get(&["MYSQL_DATABASE"])),
                None => (Some("root".to_string()), get(&["MYSQL_ROOT_PASSWORD"]), get(&["MYSQL_DATABASE"])),
            },
            Some("postgres") => (
                Some(get(&["POSTGRES_USER"]).unwrap_or_else(|| "postgres".to_string())),
                get(&["POSTGRES_PASSWORD"]),
                get(&["POSTGRES_DB"]),
            ),
            _ => (None, None, None),
        };
        for p in c.picks.iter_mut() {
            p.user = user.clone();
            p.password = pass.clone();
            p.database = db.clone();
        }
    }
}
