use skp_core::mesh::Mesh;
use skp_core::progress::{CancelToken, NoProgress, Progress, ProgressSink};
use skp_core::units::Uu;
use skp_repair::{RepairOptions, Repaired};
use skp_retopo::route::{Route, RouteChoice, RouteOptions, Routed};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

struct StageClock {
    step: &'static str,
    started: Instant,
}

impl ProgressSink for StageClock {
    fn report(&self, progress: Progress) {
        if let Progress::Measured { done, total } = progress {
            eprintln!(
                "{} stage {done}/{total} done at {:.2} s",
                self.step,
                self.started.elapsed().as_secs_f64()
            );
        }
    }
}

const USAGE: &str = "usage: skpforge-cli inspect <model.skp>\n       skpforge-cli repair <model.skp> [--weld-tolerance <cm>]\n       skpforge-cli route <model.skp> [--weld-tolerance <cm>] [--target-tris <n>] [--route a|b|auto]
       skpforge-cli retopo <model.skp> [--weld-tolerance <cm>] [--target-tris <n>] [--route a|b|auto] [--obj <dir>]";

#[derive(Debug, PartialEq)]
enum Command {
    Inspect(PathBuf),
    Repair {
        path: PathBuf,
        options: RepairOptions,
    },
    Route {
        path: PathBuf,
        options: RepairOptions,
        route: RouteOptions,
    },
    Retopo {
        path: PathBuf,
        options: RepairOptions,
        route: RouteOptions,
        obj: Option<PathBuf>,
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

fn parse_target(value: Option<String>) -> Result<usize, String> {
    let value = value.ok_or("--target-tris needs a triangle count")?;
    match value.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!(
            "--target-tris must be a positive whole number, not {value}"
        )),
    }
}

fn parse_route(value: Option<String>) -> Result<RouteChoice, String> {
    let value = value.ok_or("--route needs a, b or auto")?;
    RouteChoice::from_flag(&value).ok_or(format!("--route must be a, b or auto, not {value}"))
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let command = args.next().ok_or("no command given")?;
    let path = match command.as_str() {
        "inspect" | "repair" | "route" | "retopo" => args
            .next()
            .map(PathBuf::from)
            .ok_or(format!("{command} needs a model path"))?,
        other => return Err(format!("unknown command {other}")),
    };
    let mut options = RepairOptions::default();
    let mut route = RouteOptions::default();
    let mut obj = None;
    while let Some(arg) = args.next() {
        match (command.as_str(), arg.as_str()) {
            ("repair" | "route" | "retopo", "--weld-tolerance") => {
                options.weld_tolerance = parse_tolerance(args.next())?
            }
            ("route" | "retopo", "--target-tris") => {
                route.target_triangles = Some(parse_target(args.next())?)
            }
            ("route" | "retopo", "--route") => route.choice = parse_route(args.next())?,
            ("retopo", "--obj") => {
                obj = Some(PathBuf::from(args.next().ok_or("--obj needs a directory")?))
            }
            _ => return Err(format!("unexpected argument {arg}")),
        }
    }
    route.coplanar_tolerance = options.weld_tolerance;
    Ok(match command.as_str() {
        "inspect" => Command::Inspect(path),
        "repair" => Command::Repair { path, options },
        "route" => Command::Route {
            path,
            options,
            route,
        },
        _ => Command::Retopo {
            path,
            options,
            route,
            obj,
        },
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
        Command::Repair { path, options } => match repair(&path, &options) {
            Some(_) => ExitCode::SUCCESS,
            None => ExitCode::FAILURE,
        },
        Command::Route {
            path,
            options,
            route,
        } => match repair_and_route(&path, &options, &route) {
            Some(_) => ExitCode::SUCCESS,
            None => ExitCode::FAILURE,
        },
        Command::Retopo {
            path,
            options,
            route,
            obj,
        } => {
            let Some((repaired, routed)) = repair_and_route(&path, &options, &route) else {
                return ExitCode::FAILURE;
            };
            println!();
            let started = Instant::now();
            let (step, result) = if routed.decision.route == Route::B {
                let clock = StageClock {
                    step: "route b",
                    started,
                };
                let done = skp_retopo::route_b::route_b(
                    &repaired.mesh,
                    routed.metrics.target_triangles,
                    &CancelToken::new(),
                    &clock,
                );
                ("route b", done.map(|d| (d.to_string(), d.mesh)))
            } else {
                let clock = StageClock {
                    step: "route a",
                    started,
                };
                let done = skp_retopo::route_a::route_a(
                    &repaired.mesh,
                    routed.metrics.target_triangles,
                    route.coplanar_tolerance,
                    &CancelToken::new(),
                    &clock,
                );
                ("route a", done.map(|d| (d.to_string(), d.mesh)))
            };
            let low = match result {
                Ok((report, low)) => {
                    println!("{report}");
                    println!(
                        "{step} time (s)       {:.2}",
                        started.elapsed().as_secs_f64()
                    );
                    low
                }
                Err(e) => {
                    eprintln!("{}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            };
            if let Some(dir) = obj {
                let stem = path
                    .file_stem()
                    .map_or("model".into(), |s| s.to_string_lossy());
                for (name, mesh) in [("high", &repaired.mesh), ("low", &low)] {
                    let out = dir.join(format!("{stem}.{name}.obj"));
                    if let Err(e) = std::fs::write(&out, obj_text(mesh)) {
                        eprintln!("{}: {e}", out.display());
                        return ExitCode::FAILURE;
                    }
                    println!("wrote                  {}", out.display());
                }
            }
            ExitCode::SUCCESS
        }
    }
}

fn obj_text(mesh: &Mesh) -> String {
    let mut text = String::new();
    for p in &mesh.positions {
        text.push_str(&format!("v {} {} {}\n", p.x.0, p.y.0, p.z.0));
    }
    for face in &mesh.faces {
        text.push('f');
        for &c in face.corners() {
            text.push_str(&format!(" {}", mesh.corners[c as usize].position + 1));
        }
        text.push('\n');
    }
    text
}

fn repair_and_route(
    path: &std::path::Path,
    options: &RepairOptions,
    route: &RouteOptions,
) -> Option<(Repaired, Routed)> {
    let repaired = repair(path, options)?;
    println!();
    match skp_retopo::route::route(&repaired.mesh, route) {
        Ok(routed) => {
            println!("{routed}");
            Some((repaired, routed))
        }
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            None
        }
    }
}

fn repair(path: &std::path::Path, options: &RepairOptions) -> Option<Repaired> {
    let cancel = CancelToken::new();
    let import = match skp_io::import(path, &cancel, &NoProgress) {
        Ok(import) => import,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return None;
        }
    };
    println!("{}", import.report);
    println!();
    let started = Instant::now();
    match skp_repair::repair(
        &import.mesh,
        options,
        &cancel,
        &StageClock {
            step: "repair",
            started,
        },
    ) {
        Ok(repaired) => {
            println!("weld tolerance (cm)    {}", options.weld_tolerance.0);
            println!("{}", repaired.report);
            println!(
                "repair time (s)        {:.2}",
                started.elapsed().as_secs_f64()
            );
            Some(repaired)
        }
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            None
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
    fn route_takes_a_target_and_a_forced_route() {
        assert_eq!(
            parse(args(&["route", "house.skp"])),
            Ok(Command::Route {
                path: PathBuf::from("house.skp"),
                options: RepairOptions::default(),
                route: RouteOptions::default(),
            })
        );
        assert_eq!(
            parse(args(&[
                "route",
                "house.skp",
                "--target-tris",
                "5000",
                "--route",
                "b",
                "--weld-tolerance",
                "0.01"
            ])),
            Ok(Command::Route {
                path: PathBuf::from("house.skp"),
                options: RepairOptions {
                    weld_tolerance: Uu(0.01)
                },
                route: RouteOptions {
                    target_triangles: Some(5000),
                    choice: RouteChoice::B,
                    coplanar_tolerance: Uu(0.01),
                },
            })
        );
        assert!(parse(args(&["route", "house.skp", "--target-tris", "0"])).is_err());
        assert!(parse(args(&["route", "house.skp", "--target-tris", "many"])).is_err());
        assert!(parse(args(&["route", "house.skp", "--route", "c"])).is_err());
        assert!(parse(args(&["repair", "house.skp", "--target-tris", "10"])).is_err());
    }

    #[test]
    fn retopo_takes_the_same_flags_as_route() {
        assert_eq!(
            parse(args(&[
                "retopo",
                "house.skp",
                "--target-tris",
                "800",
                "--route",
                "a"
            ])),
            Ok(Command::Retopo {
                path: PathBuf::from("house.skp"),
                options: RepairOptions::default(),
                route: RouteOptions {
                    target_triangles: Some(800),
                    choice: RouteChoice::A,
                    ..RouteOptions::default()
                },
                obj: None,
            })
        );
        assert!(matches!(
            parse(args(&["retopo", "house.skp", "--obj", "out"])),
            Ok(Command::Retopo { obj: Some(dir), .. }) if dir.as_os_str() == "out"
        ));
        assert!(parse(args(&["retopo", "house.skp", "--obj"])).is_err());
        assert!(parse(args(&["route", "house.skp", "--obj", "out"])).is_err());
        assert!(parse(args(&["retopo"])).is_err());
        assert!(parse(args(&["retopo", "house.skp", "--route", "x"])).is_err());
    }

    #[test]
    fn obj_text_numbers_positions_from_one_and_keeps_quads() {
        let mesh = Mesh {
            positions: vec![
                skp_core::mesh::Point::new(0.0, 0.0, 0.0),
                skp_core::mesh::Point::new(1.0, 0.0, 0.0),
                skp_core::mesh::Point::new(1.0, 1.0, 0.0),
                skp_core::mesh::Point::new(0.0, 1.0, 0.0),
            ],
            corners: (0..4)
                .map(|p| skp_core::mesh::Corner {
                    position: p,
                    ..Default::default()
                })
                .collect(),
            faces: vec![Face::Quad([0, 1, 2, 3])],
            ..Mesh::default()
        };
        assert_eq!(
            obj_text(&mesh),
            "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n"
        );
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
