use crate::geometry::{area_vector, triangle_positions, Vec3};
use crate::topology::{triangle_edges, Edges};
use skp_core::mesh::{Face, Mesh};
use skp_core::progress::{CancelToken, Cancelled};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Orientation {
    pub turned_over: usize,
    pub components: usize,
    pub closed_components: usize,
}

pub fn orient(mesh: &mut Mesh, cancel: &CancelToken) -> Result<Orientation, Cancelled> {
    let triangles: Vec<[u32; 3]> = (0..mesh.faces.len())
        .map(|face| triangle_positions(mesh, face))
        .collect();
    let edges = Edges::build(&triangles);
    let mut flip: Vec<Option<bool>> = vec![None; triangles.len()];
    let mut outcome = Orientation::default();
    for seed in 0..triangles.len() {
        if flip[seed].is_some() {
            continue;
        }
        cancel.check()?;
        let component = walk(seed, &triangles, &edges, &mut flip);
        outcome.components += 1;
        let closed = component.iter().all(|&t| {
            triangle_edges(triangles[t])
                .iter()
                .all(|&(a, b)| edges.degree(a, b) == 2)
        });
        let invert = if closed {
            outcome.closed_components += 1;
            signed_volume(mesh, &triangles, &component, &flip) < 0.0
        } else {
            let (turned, kept) = component.iter().fold((0.0, 0.0), |(turned, kept), &t| {
                let area = area_of(mesh, triangles[t]);
                if flip[t] == Some(true) {
                    (turned + area, kept)
                } else {
                    (turned, kept + area)
                }
            });
            turned > kept
        };
        if invert {
            for &t in &component {
                flip[t] = flip[t].map(|f| !f);
            }
        }
    }
    for (face, f) in flip.iter().enumerate() {
        if *f == Some(true) {
            turn_over(mesh, face);
            outcome.turned_over += 1;
        }
    }
    Ok(outcome)
}

fn walk(
    seed: usize,
    triangles: &[[u32; 3]],
    edges: &Edges,
    flip: &mut [Option<bool>],
) -> Vec<usize> {
    flip[seed] = Some(false);
    let mut stack = vec![seed];
    let mut component = Vec::new();
    while let Some(t) = stack.pop() {
        component.push(t);
        let turned = flip[t] == Some(true);
        for (a, b) in triangle_edges(triangles[t]) {
            let Some(other) = edges.other(a, b, t as u32) else {
                continue;
            };
            let other_face = other.face as usize;
            if flip[other_face].is_some() {
                continue;
            }
            let mine = a < b;
            let wanted = !(mine ^ turned);
            flip[other_face] = Some(wanted ^ other.forward);
            stack.push(other_face);
        }
    }
    component
}

fn points_of(mesh: &Mesh, tri: [u32; 3]) -> [Vec3; 3] {
    tri.map(|p| Vec3::of(mesh.positions[p as usize]))
}

fn area_of(mesh: &Mesh, tri: [u32; 3]) -> f64 {
    let [a, b, c] = points_of(mesh, tri);
    area_vector(a, b, c).length()
}

fn signed_volume(
    mesh: &Mesh,
    triangles: &[[u32; 3]],
    component: &[usize],
    flip: &[Option<bool>],
) -> f64 {
    let origin = points_of(mesh, triangles[component[0]])[0];
    component
        .iter()
        .map(|&t| {
            let [a, b, c] = points_of(mesh, triangles[t]).map(|p| p - origin);
            let v = a.dot(b.cross(c));
            if flip[t] == Some(true) {
                -v
            } else {
                v
            }
        })
        .sum()
}

fn turn_over(mesh: &mut Mesh, face: usize) {
    let c = mesh.faces[face].corners();
    let [c0, c1, c2] = [c[0], c[1], c[2]];
    let first = mesh.corners.len() as u32;
    for corner in [c0, c2, c1] {
        let turned = mesh.corners[corner as usize].turned_over();
        mesh.corners.push(turned);
    }
    mesh.faces[face] = Face::Tri([first, first + 1, first + 2]);
    mesh.face_data[face] = mesh.face_data[face].turned_over();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{cube, indexed, position_triangles, reversed};
    use skp_core::mesh::{Material, MaterialId, Point};

    fn run(mesh: &mut Mesh) -> Orientation {
        orient(mesh, &CancelToken::new()).unwrap()
    }

    fn consistent(mesh: &Mesh) -> bool {
        let tris = position_triangles(mesh);
        let edges = Edges::build(&tris);
        tris.iter().all(|&tri| {
            triangle_edges(tri)
                .iter()
                .all(|&(a, b)| match edges.around(a, b) {
                    [x, y] => x.forward != y.forward,
                    _ => true,
                })
        })
    }

    fn volume(mesh: &Mesh) -> f64 {
        position_triangles(mesh)
            .iter()
            .map(|&tri| {
                let [a, b, c] = points_of(mesh, tri);
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }

    #[test]
    fn a_consistent_outward_cube_is_left_alone() {
        let (points, tris) = cube(2.0);
        let mut mesh = indexed(&points, &tris);
        let outcome = run(&mut mesh);
        assert_eq!(outcome.turned_over, 0);
        assert_eq!(outcome.components, 1);
        assert_eq!(outcome.closed_components, 1);
    }

    #[test]
    fn a_reversed_side_of_a_cube_is_turned_back() {
        let (points, mut tris) = cube(2.0);
        tris[4] = reversed(tris[4]);
        tris[5] = reversed(tris[5]);
        let mut mesh = indexed(&points, &tris);
        assert_eq!(run(&mut mesh).turned_over, 2);
        assert!(consistent(&mesh));
        assert!((volume(&mesh) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn an_inside_out_cube_is_turned_outward_even_though_it_was_consistent() {
        let (points, tris) = cube(2.0);
        let tris: Vec<_> = tris.into_iter().map(reversed).collect();
        let mut mesh = indexed(&points, &tris);
        assert_eq!(run(&mut mesh).turned_over, 12);
        assert!((volume(&mesh) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn an_open_surface_keeps_the_winding_of_most_of_its_area() {
        let points = [
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
            Point::new(1.0, 1.0, 0.0),
            Point::new(2.0, 0.0, 0.0),
        ];
        let mut mesh = indexed(&points, &[[0, 1, 2], [2, 3, 1], [1, 4, 3]]);
        assert_eq!(run(&mut mesh).turned_over, 1);
        assert!(consistent(&mesh));
        let normal_z = |face: usize| {
            let [a, b, c] = points_of(&mesh, position_triangles(&mesh)[face]);
            area_vector(a, b, c).z
        };
        assert!((0..3).all(|f| normal_z(f) > 0.0));
    }

    #[test]
    fn turning_over_swaps_materials_and_uvq_sides() {
        let points = [
            Point::new(0.0, 0.0, 0.0),
            Point::new(4.0, 0.0, 0.0),
            Point::new(0.0, 4.0, 0.0),
            Point::new(3.0, 3.0, 0.0),
        ];
        let mut mesh = indexed(&points, &[[0, 1, 2], [1, 2, 3]]);
        mesh.face_data[1].front = Some(MaterialId(0));
        mesh.face_data[1].back = Some(MaterialId(1));
        mesh.materials = vec![Material::default(), Material::default()];
        let front_uvq = mesh.corners[3].uvq;
        let back_uvq = mesh.corners[3].back_uvq;
        run(&mut mesh);
        let data = mesh.face_data[1];
        assert_eq!(data.front, Some(MaterialId(1)));
        assert_eq!(data.back, Some(MaterialId(0)));
        let turned = mesh.faces[1].corners()[0] as usize;
        assert_eq!(mesh.corners[turned].uvq, back_uvq);
        assert_eq!(mesh.corners[turned].back_uvq, front_uvq);
        assert!(mesh.corners[turned].normal.z > 0.0);
        assert_eq!(mesh.validate(), Ok(()));
    }

    #[test]
    fn a_non_manifold_edge_does_not_carry_orientation_across() {
        let points = [
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.5, 1.0, 0.0),
            Point::new(0.5, -1.0, 0.0),
            Point::new(0.5, 0.0, 1.0),
        ];
        let mut mesh = indexed(&points, &[[0, 1, 2], [0, 1, 3], [0, 1, 4]]);
        let outcome = run(&mut mesh);
        assert_eq!(outcome.components, 3);
        assert_eq!(outcome.turned_over, 0);
    }

    #[test]
    fn cancellation_is_honoured() {
        let (points, tris) = cube(1.0);
        let mut mesh = indexed(&points, &tris);
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(orient(&mut mesh, &cancel), Err(Cancelled));
    }
}
