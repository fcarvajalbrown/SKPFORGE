use skp_core::mesh::{Corner, Face, FaceData, Mesh, Point};
use skp_core::progress::{CancelToken, NoProgress};
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::Instant;

fn read_obj(text: &str) -> Result<Mesh, String> {
    let mut mesh = Mesh::default();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => {
                let c: Vec<f64> = parts
                    .take(3)
                    .map(|t| t.parse::<f64>().map_err(|e| format!("{line}: {e}")))
                    .collect::<Result<_, _>>()?;
                if c.len() != 3 {
                    return Err(format!("{line}: needs three coordinates"));
                }
                mesh.positions.push(Point::new(c[0], c[1], c[2]));
            }
            Some("f") => {
                let ids: Vec<u32> = parts
                    .map(|t| {
                        let index = t.split('/').next().unwrap_or(t);
                        index
                            .parse::<u32>()
                            .map(|i| i - 1)
                            .map_err(|e| format!("{line}: {e}"))
                    })
                    .collect::<Result<_, _>>()?;
                let triangles: Vec<[u32; 3]> = match ids.len() {
                    3 => vec![[ids[0], ids[1], ids[2]]],
                    4 => vec![[ids[0], ids[1], ids[2]], [ids[3], ids[0], ids[2]]],
                    n => return Err(format!("{line}: {n} corners, expected 3 or 4")),
                };
                for t in triangles {
                    let base = mesh.corners.len() as u32;
                    for p in t {
                        mesh.corners.push(Corner {
                            position: p,
                            ..Corner::default()
                        });
                    }
                    mesh.faces.push(Face::Tri([base, base + 1, base + 2]));
                    mesh.face_data.push(FaceData::default());
                }
            }
            _ => {}
        }
    }
    Ok(mesh)
}

fn write_obj(mesh: &Mesh) -> String {
    let mut text = String::new();
    for p in &mesh.positions {
        let _ = writeln!(text, "v {} {} {}", p.x.0, p.y.0, p.z.0);
    }
    for face in &mesh.faces {
        text.push('f');
        for &c in face.corners() {
            let _ = write!(text, " {}", mesh.corners[c as usize].position + 1);
        }
        text.push('\n');
    }
    text
}

fn run(args: &[String]) -> Result<(), String> {
    let [input, output, quads] = args else {
        return Err("usage: route_b_obj <in.obj> <out.obj> <quads>".into());
    };
    let quads: usize = quads
        .parse()
        .map_err(|e| format!("quads must be a whole number: {e}"))?;
    let text = std::fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
    let mesh = read_obj(&text)?;
    let started = Instant::now();
    let done = skp_retopo::route_b::route_b(&mesh, quads * 2, &CancelToken::new(), &NoProgress)
        .map_err(|e| e.to_string())?;
    let seconds = started.elapsed().as_secs_f64();
    std::fs::write(output, write_obj(&done.mesh)).map_err(|e| format!("{output}: {e}"))?;
    println!("{done}");
    println!("route b time (s)       {seconds:.2}");
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
