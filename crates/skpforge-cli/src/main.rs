use skp_core::mesh::Mesh;
use skp_core::progress::{CancelToken, NoProgress, Progress, ProgressSink};
use skp_core::units::Uu;
use skp_repair::RepairOptions;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

struct StageClock(Instant);

impl ProgressSink for StageClock {
    fn report(&self, progress: Progress) {
        if let Progress::Measured { done, total } = progress {
            eprintln!(
                "repair stage {done}/{total} done at {:.2} s",
                self.0.elapsed().as_secs_f64()
            );
        }
    }
}

const USAGE: &str = "usage: skpforge-cli inspect <model.skp>\n       skpforge-cli repair <model.skp> [--weld-tolerance <cm>]";

#[derive(Debug, PartialEq)]
enum Command {
    Inspect(PathBuf),
    Repair {
        path: PathBuf,
        options: RepairOptions,
    },
}

fn parse_tolerance(value: Option<String>) -> Result<Uu, String> {
    let value = value.ok_or("--weld-tolerance needs a value in centimetres")?;
    match value.parse::<f64>() {
        Ok(cm) if cm >= 0.0 && cm.is_finite() => Ok(Uu(cm)),
        _ => Err(format!(
            "--weld-tolerance must be a non-negative number of centimetres, not {value}"
        )),
    }
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let command = args.next().ok_or("no command given")?;
    let path = match command.as_str() {
        "inspect" | "repair" => args
            .next()
            .map(PathBuf::from)
            .ok_or(format!("{command} needs a model path"))?,
        other => return Err(format!("unknown command {other}")),
    };
    let mut options = RepairOptions::default();
    while let Some(arg) = args.next() {
        match (command.as_str(), arg.as_str()) {
            ("repair", "--weld-tolerance") => {
                options.weld_tolerance = parse_tolerance(args.next())?
            }
            _ => return Err(format!("unexpected argument {arg}")),
        }
    }
    Ok(match command.as_str() {
        "inspect" => Command::Inspect(path),
        _ => Command::Repair { path, options },
    })
}

fn material_table(mesh: &Mesh) -> String {
    let mut front = vec![0usize; mesh.materials.len()];
    let mut back = vec![0usize; mesh.materials.len()];
    for data in &mesh.face_data {
        if let Some(id) = data.front {
            front[id.0 as usize] += 1;
        }
        if let Some(id) = data.back {
            back[id.0 as usize] += 1;
        }
    }
    let mut out = String::from("material                         front tris  back tris\n");
    for (i, m) in mesh.materials.iter().enumerate() {
        out.push_str(&format!(
            "{:<32} {:>10} {:>10}\n",
            m.name, front[i], back[i]
        ));
    }
    out
}

fn main() -> ExitCode {
    let command = match parse(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match command {
        Command::Inspect(path) => match skp_io::import(&path, &CancelToken::new(), &NoProgress) {
            Ok(import) => {
                println!("{}", import.report);
                println!();
                print!("{}", material_table(&import.mesh));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                ExitCode::FAILURE
            }
        },
        Command::Repair { path, options } => repair(&path, &options),
    }
}

fn repair(path: &std::path::Path, options: &RepairOptions) -> ExitCode {
    let cancel = CancelToken::new();
    let import = match skp_io::import(path, &cancel, &NoProgress) {
        Ok(import) => import,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    println!("{}", import.report);
    println!();
    let started = Instant::now();
    match skp_repair::repair(&import.mesh, options, &cancel, &StageClock(started)) {
        Ok(repaired) => {
            println!("weld tolerance (cm)    {}", options.weld_tolerance.0);
            println!("{}", repaired.report);
            println!(
                "repair time (s)        {:.2}",
                started.elapsed().as_secs_f64()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skp_core::mesh::{Face, FaceData, Material, MaterialId};

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn inspect_takes_exactly_one_path() {
        assert_eq!(
            parse(args(&["inspect", "house.skp"])),
            Ok(Command::Inspect(PathBuf::from("house.skp")))
        );
        assert!(parse(args(&["inspect"])).is_err());
        assert!(parse(args(&["inspect", "a.skp", "b.skp"])).is_err());
        assert!(parse(args(&["export"])).is_err());
        assert!(parse(args(&[])).is_err());
    }

    #[test]
    fn repair_defaults_to_the_adr_weld_tolerance() {
        assert_eq!(
            parse(args(&["repair", "house.skp"])),
            Ok(Command::Repair {
                path: PathBuf::from("house.skp"),
                options: RepairOptions::default(),
            })
        );
    }

    #[test]
    fn repair_takes_a_weld_tolerance_override_in_centimetres() {
        assert_eq!(
            parse(args(&["repair", "house.skp", "--weld-tolerance", "0.01"])),
            Ok(Command::Repair {
                path: PathBuf::from("house.skp"),
                options: RepairOptions {
                    weld_tolerance: Uu(0.01)
                },
            })
        );
        assert!(parse(args(&["repair", "house.skp", "--weld-tolerance"])).is_err());
        assert!(parse(args(&["repair", "house.skp", "--weld-tolerance", "-1"])).is_err());
        assert!(parse(args(&["repair", "house.skp", "--weld-tolerance", "wide"])).is_err());
        assert!(parse(args(&["inspect", "house.skp", "--weld-tolerance", "1"])).is_err());
        assert!(parse(args(&["repair"])).is_err());
    }

    #[test]
    fn material_table_counts_front_and_back_separately() {
        let mesh = Mesh {
            faces: vec![Face::Tri([0, 0, 0]); 2],
            face_data: vec![
                FaceData {
                    front: Some(MaterialId(0)),
                    back: Some(MaterialId(1)),
                    ..FaceData::default()
                },
                FaceData {
                    front: Some(MaterialId(0)),
                    back: None,
                    ..FaceData::default()
                },
            ],
            materials: vec![
                Material {
                    name: "Brick".into(),
                },
                Material {
                    name: "Glass".into(),
                },
            ],
            ..Mesh::default()
        };
        let table = material_table(&mesh);
        let rows: Vec<Vec<&str>> = table
            .lines()
            .skip(1)
            .map(|l| l.split_whitespace().collect())
            .collect();
        assert_eq!(rows, vec![vec!["Brick", "2", "0"], vec!["Glass", "0", "1"]]);
    }
}
