use skp_core::mesh::Mesh;
use skp_core::progress::{CancelToken, NoProgress};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: skpforge-cli inspect <model.skp>";

#[derive(Debug, PartialEq)]
enum Command {
    Inspect(PathBuf),
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    match (args.next().as_deref(), args.next(), args.next()) {
        (Some("inspect"), Some(path), None) => Ok(Command::Inspect(PathBuf::from(path))),
        (Some("inspect"), None, _) => Err("inspect needs a model path".into()),
        (Some("inspect"), Some(_), Some(extra)) => Err(format!("unexpected argument {extra}")),
        (Some(other), _, _) => Err(format!("unknown command {other}")),
        (None, _, _) => Err("no command given".into()),
    }
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
