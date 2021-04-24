use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_yaml::Value;
use std::fs;
use url::Url;

mod cli;

const STATUS_FILE: &str = "/var/lib/apt/gen/status.json";
const REPO_MIRROR_FILE: &str = "/usr/share/distro-repository-data/mirrors.yml";
const REPO_COMPONENT_FILE: &str = "/usr/share/distro-repository-data/comps.yml";
const REPO_BRANCH_FILE: &str = "/usr/share/distro-repository-data/branches.yml";
const APT_SOURCE_FILE: &str = "/etc/apt/sources.list";

#[derive(Deserialize, Serialize)]
struct Status {
    branch: String,
    component: Vec<String>,
    mirror: Vec<String>,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            branch: "stable".to_string(),
            component: vec!["main".to_string()],
            mirror: vec!["origin".to_string()],
        }
    }
}
fn main() -> Result<()> {
    let app = cli::build_cli().get_matches();
    let mut status = read_status()?;

    match app.subcommand() {
        ("status", _) => {
            println!("Branch: {}", status.branch);
            for i in status.component {
                println!("component: {}", i);
            }
            for i in status.mirror {
                println!("mirror: {}", i);
            }
        }
        ("set-mirror", Some(args)) => {
            let new_mirror = args.value_of("INPUT").unwrap();
            status.mirror = vec![new_mirror.to_string()];
            let result = to_config(&status)?;
            apply_config(&status, result)?;
        }
        ("add-mirror", Some(args)) => {
            add_mirror(args, &mut status)?;
        }
        ("remove-mirror", Some(args)) => {
            remove_mirror(args, &mut status)?;
        }
        ("add-component", Some(args)) => {
            add_component(args, &mut status)?;
        }
        ("remove-component", Some(args)) => {
            remove_component(args, status)?;
        }
        ("set-branch", Some(args)) => {
            let new_branch = args.value_of("INPUT").unwrap();
            if read_distro_file(REPO_BRANCH_FILE)?
                .get(new_branch)
                .is_some()
            {
                status.branch = new_branch.to_string();
            } else {
                return Err(anyhow!("branch doesn't exist!"));
            }
        }
        _ => {
            unreachable!()
        }
    }

    Ok(())
}

fn remove_mirror(args: &clap::ArgMatches, status: &mut Status) -> Result<(), anyhow::Error> {
    if status.mirror.len() == 1 {
        return Err(anyhow!("only have 1 mirror! cannot delete it!!!"));
    }
    for i in args.values_of("INPUT").unwrap() {
        if let Some(index) = status.mirror.iter().position(|v| v == i) {
            status.mirror.remove(index);
        } else {
            return Err(anyhow!("Cannot find mirror: {}", i));
        }
    }
    let result = to_config(&status)?;
    apply_config(&*status, result)?;

    Ok(())
}

fn add_mirror(args: &clap::ArgMatches, status: &mut Status) -> Result<(), anyhow::Error> {
    for i in args.values_of("INPUT").unwrap() {
        if status.mirror.contains(&i.to_string()) {
            return Err(anyhow!("mirror already exist!"));
        } else {
            status.mirror.push(i.to_string());
        }
    }
    let result = to_config(&status)?;
    apply_config(&*status, result)?;

    Ok(())
}

fn remove_component(args: &clap::ArgMatches, mut status: Status) -> Result<(), anyhow::Error> {
    for i in args.values_of("INPUT").unwrap() {
        if let Some(index) = status.component.iter().position(|v| v == i) {
            status.component.remove(index);
        } else {
            return Err(anyhow!("Component: {} doesn't exist in component.", &i));
        }
    }

    let result = to_config(&status)?;
    apply_config(&status, result)?;

    Ok(())
}

fn add_component(args: &clap::ArgMatches, status: &mut Status) -> Result<(), anyhow::Error> {
    for i in args.values_of("INPUT").unwrap() {
        if status.component.contains(&i.to_string()) {
            return Err(anyhow!("{} already exist in component.", &i));
        } else if read_distro_file(REPO_COMPONENT_FILE)?.get(i).is_none() {
            return Err(anyhow!("{} is not option.", &i));
        } else {
            status.component.push(i.to_string());
        }
    }
    let result = to_config(&status)?;
    apply_config(&status, result)?;

    Ok(())
}

fn read_status() -> Result<Status> {
    let status = match fs::read(STATUS_FILE) {
        Ok(v) => v,
        Err(_) => {
            fs::create_dir_all("/var/lib/apt/gen")?;
            fs::File::create(STATUS_FILE)?;
            fs::read(STATUS_FILE)?
        }
    };
    let status: Status = serde_json::from_slice(&status).unwrap_or_default();

    Ok(status)
}

fn read_distro_file(file: &str) -> Result<Value> {
    if let Ok(mirrors_data) = fs::read(file) {
        return Ok(serde_yaml::from_slice(&mirrors_data)?);
    }

    Err(anyhow!(
        "distro repo data not found! Pleease check your aosc-os-repository-data package."
    ))
}

fn apply_config(status: &Status, source_list_str: String) -> Result<()> {
    fs::write(
        STATUS_FILE,
        format!("{} \n", serde_json::to_string(&status)?),
    )?;
    fs::write(APT_SOURCE_FILE, source_list_str)?;

    Ok(())
}

fn to_config(status: &Status) -> Result<String> {
    let mut result = "".to_string();
    for i in &status.mirror {
        let mirror_url = get_mirror_url(i.as_str())?;
        let debs_url = Url::parse(&mirror_url)?.join("debs")?;
        for branch in get_branch_suites(&status.branch)? {
            result += &format!(
                "deb {} {} {} \n",
                debs_url.as_str(),
                branch,
                status.component.join(" ")
            );
        }
    }

    Ok(result)
}

fn get_mirror_url(mirror_name: &str) -> Result<String> {
    if Url::parse(mirror_name).is_ok() {
        return Ok(mirror_name.to_string());
    }
    let mirror_url = read_distro_file(REPO_MIRROR_FILE)?
        .get(mirror_name)
        .ok_or_else(|| anyhow!("mirror doesn't exist!"))?
        .get("url")
        .ok_or_else(|| anyhow!("No url found on value!"))?
        .as_str()
        .ok_or_else(|| anyhow!("Url isn't String!"))?
        .to_owned();

    Ok(mirror_url)
}

fn get_branch_suites(branch_name: &str) -> Result<Vec<String>> {
    let branch_suites = read_distro_file(REPO_BRANCH_FILE)?
        .get(branch_name)
        .ok_or_else(|| anyhow!("branch doesn't exist!"))?
        .get("suites")
        .ok_or_else(|| {
            anyhow!("suites doesn't exist! Please check your aosc-os-repository-data package!")
        })?
        .as_sequence()
        .ok_or_else(|| {
            anyhow!("suites isn't arrays! Please check your aosc-os-repository-data package!")
        })?
        .to_owned();

    let mut suites = Vec::new();
    for i in branch_suites {
        if let Some(i) = i.as_str() {
            suites.push(i.to_string());
        } else {
            return Err(anyhow!(
                "suites broken! Please check your aosc-os-repository-data package!"
            ));
        }
    }
    Ok(suites)
}
