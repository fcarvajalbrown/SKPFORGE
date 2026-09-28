use crate::error::IoError;
use crate::report::ImportReport;
use crate::scene::{Node, NodeKind, Scene, SourceFace, Transform};
use skp_core::mesh::{Corner, Face, FaceData, MaterialId, Mesh, Normal, Point, Uvq};
use skp_core::progress::{CancelToken, Progress, ProgressSink};
use skp_core::units::{Inches, Uu};
use std::collections::BTreeSet;

pub fn q_variance(uvqs: impl IntoIterator<Item = Uvq>) -> f64 {
    let qs: Vec<f64> = uvqs.into_iter().map(|t| t.q).collect();
    if qs.is_empty() {
        return 0.0;
    }
    if qs.iter().any(|&q| q == 0.0 || !q.is_finite()) {
        return f64::INFINITY;
    }
    let mean = qs.iter().sum::<f64>() / qs.len() as f64;
    if mean == 0.0 {
        return f64::INFINITY;
    }
    qs.iter().map(|q| (q / mean - 1.0).powi(2)).sum::<f64>() / qs.len() as f64
}

pub fn resolve_material(
    own: Option<MaterialId>,
    inherited: Option<MaterialId>,
) -> Option<MaterialId> {
    own.or(inherited)
}

struct Walker<'a> {
    mesh: Mesh,
    report: ImportReport,
    used: BTreeSet<MaterialId>,
    cancel: &'a CancelToken,
    progress: &'a dyn ProgressSink,
    total: u64,
}

pub fn flatten(
    scene: &Scene,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
) -> Result<(Mesh, ImportReport), IoError> {
    let mut walker = Walker {
        mesh: Mesh {
            materials: scene.materials.clone(),
            ..Mesh::default()
        },
        report: ImportReport {
            load_status: scene.load_status,
            units: scene.units,
            hidden_skipped: scene.hidden_skipped,
            materials_in_model: scene.materials.len(),
            ..ImportReport::default()
        },
        used: BTreeSet::new(),
        cancel,
        progress,
        total: scene.root.face_count() as u64,
    };
    walker.node(&scene.root, &Transform::IDENTITY, None)?;
    let Walker {
        mesh,
        mut report,
        used,
        ..
    } = walker;
    report.materials_used = used.len();
    report.triangles = mesh.triangle_count();
    report.bounds = bounds(&mesh.positions);
    mesh.validate()?;
    Ok((mesh, report))
}

impl Walker<'_> {
    fn node(
        &mut self,
        node: &Node,
        parent: &Transform,
        inherited: Option<MaterialId>,
    ) -> Result<(), IoError> {
        let world = parent.then_apply(&node.transform);
        match node.kind {
            NodeKind::Root => {}
            NodeKind::Group => self.report.groups += 1,
            NodeKind::Instance => self.report.instances += 1,
        }
        if node.kind != NodeKind::Root && world.is_mirroring() {
            self.report.mirrored_nodes += 1;
        }
        let inherited = resolve_material(node.material, inherited);
        for face in &node.faces {
            self.cancel.check()?;
            self.face(face, &world, inherited)?;
            self.progress.report(Progress::Measured {
                done: self.report.faces as u64,
                total: self.total,
            });
        }
        for child in &node.children {
            self.node(child, &world, inherited)?;
        }
        Ok(())
    }

    fn face(
        &mut self,
        face: &SourceFace,
        world: &Transform,
        inherited: Option<MaterialId>,
    ) -> Result<(), IoError> {
        let index = self.report.faces;
        let data = FaceData {
            front: resolve_material(face.front, inherited),
            back: resolve_material(face.back, inherited),
            q_variance: 0.0,
        };
        let back_only = data.is_back_only();
        let q_variance =
            q_variance(
                face.vertices
                    .iter()
                    .map(|v| if back_only { v.back } else { v.front }),
            );
        let data = FaceData { q_variance, ..data };

        let first_corner = self.mesh.corners.len() as u32;
        for v in &face.vertices {
            let inches = [v.position[0].0, v.position[1].0, v.position[2].0];
            let p = world
                .apply_point(inches)
                .ok_or(IoError::DegenerateTransform { face: index })?;
            let n = world.apply_normal(v.normal);
            let position = self.mesh.positions.len() as u32;
            self.mesh.positions.push(Point {
                x: Uu::from(Inches(p[0])),
                y: Uu::from(Inches(p[1])),
                z: Uu::from(Inches(p[2])),
            });
            self.mesh.corners.push(Corner {
                position,
                uvq: v.front,
                back_uvq: v.back,
                normal: Normal {
                    x: n[0],
                    y: n[1],
                    z: n[2],
                },
            });
        }
        let mirrored = world.is_mirroring();
        for &[a, b, c] in &face.triangles {
            let tri = if mirrored { [a, c, b] } else { [a, b, c] };
            self.mesh
                .faces
                .push(Face::Tri(tri.map(|i| first_corner + i)));
            self.mesh.face_data.push(data);
        }

        self.report.faces += 1;
        self.used.extend(data.front);
        self.used.extend(data.back);
        if data.front.is_none() && data.back.is_none() {
            self.report.default_material_faces += 1;
        }
        if back_only {
            self.report.back_only_faces += 1;
        }
        if q_variance.is_infinite() {
            self.report.zero_q_faces += 1;
        } else if q_variance > 0.0 {
            self.report.non_constant_q_faces += 1;
            self.report.max_q_variance = self.report.max_q_variance.max(q_variance);
        }
        Ok(())
    }
}

fn bounds(points: &[Point]) -> Option<(Point, Point)> {
    let first = *points.first()?;
    Some(points.iter().fold((first, first), |(lo, hi), p| {
        (
            Point {
                x: Uu(lo.x.0.min(p.x.0)),
                y: Uu(lo.y.0.min(p.y.0)),
                z: Uu(lo.z.0.min(p.z.0)),
            },
            Point {
                x: Uu(hi.x.0.max(p.x.0)),
                y: Uu(hi.y.0.max(p.y.0)),
                z: Uu(hi.z.0.max(p.z.0)),
            },
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SourceVertex;
    use skp_core::mesh::Material;
    use skp_core::progress::NoProgress;

    fn uvq(u: f64, v: f64, q: f64) -> Uvq {
        Uvq { u, v, q }
    }

    fn unit_triangle(front: Option<MaterialId>, back: Option<MaterialId>) -> SourceFace {
        let vertex = |x: f64, y: f64| SourceVertex {
            position: [Inches(x), Inches(y), Inches(0.0)],
            normal: [0.0, 0.0, 1.0],
            front: uvq(x, y, 1.0),
            back: uvq(-x, y, 1.0),
        };
        SourceFace {
            vertices: vec![vertex(0.0, 0.0), vertex(1.0, 0.0), vertex(0.0, 1.0)],
            triangles: vec![[0, 1, 2]],
            front,
            back,
        }
    }

    fn scene(root: Node) -> Scene {
        Scene {
            materials: vec![
                Material {
                    name: "Brick".into(),
                },
                Material {
                    name: "Paint".into(),
                },
            ],
            root,
            ..Scene::default()
        }
    }

    fn run(scene: &Scene) -> (Mesh, ImportReport) {
        flatten(scene, &CancelToken::new(), &NoProgress).unwrap()
    }

    fn signed_z(mesh: &Mesh, face: usize) -> f64 {
        let c = mesh.faces[face].corners();
        let p = |i: usize| mesh.corner_position(c[i]).unwrap();
        let (a, b, d) = (p(0), p(1), p(2));
        let (ux, uy) = ((b.x - a.x).0, (b.y - a.y).0);
        let (vx, vy) = ((d.x - a.x).0, (d.y - a.y).0);
        ux * vy - uy * vx
    }

    #[test]
    fn constant_q_has_zero_variance_even_when_not_one() {
        assert_eq!(q_variance([uvq(1.0, 0.0, 2.0), uvq(0.0, 1.0, 2.0)]), 0.0);
    }

    #[test]
    fn varying_q_has_positive_variance() {
        assert!(q_variance([uvq(0.0, 0.0, 1.0), uvq(0.0, 0.0, 3.0)]) > 0.0);
    }

    #[test]
    fn exactly_zero_q_is_infinite_rather_than_divided() {
        assert_eq!(
            q_variance([uvq(0.0, 0.0, 1.0), uvq(0.0, 0.0, 0.0)]),
            f64::INFINITY
        );
    }

    #[test]
    fn inches_become_centimetres_at_the_boundary() {
        let root = Node {
            children: vec![Node {
                kind: NodeKind::Group,
                transform: Transform::translation(10.0, 0.0, 0.0),
                faces: vec![unit_triangle(None, None)],
                ..Node::default()
            }],
            ..Node::default()
        };
        let (mesh, _) = run(&scene(root));
        assert_eq!(mesh.positions[0], Point::new(25.4, 0.0, 0.0));
        assert_eq!(mesh.positions[1], Point::new(11.0 * 2.54, 0.0, 0.0));
    }

    #[test]
    fn nested_transforms_compose_parent_first() {
        let root = Node {
            children: vec![Node {
                kind: NodeKind::Instance,
                transform: Transform::translation(0.0, 0.0, 100.0),
                children: vec![Node {
                    kind: NodeKind::Group,
                    transform: Transform::scale(2.0, 2.0, 2.0),
                    faces: vec![unit_triangle(None, None)],
                    ..Node::default()
                }],
                ..Node::default()
            }],
            ..Node::default()
        };
        let (mesh, report) = run(&scene(root));
        assert_eq!(mesh.positions[1], Point::new(2.0 * 2.54, 0.0, 100.0 * 2.54));
        assert_eq!((report.groups, report.instances), (1, 1));
    }

    #[test]
    fn face_material_wins_over_the_ancestor() {
        let root = Node {
            children: vec![Node {
                kind: NodeKind::Group,
                material: Some(MaterialId(1)),
                faces: vec![unit_triangle(Some(MaterialId(0)), None)],
                ..Node::default()
            }],
            ..Node::default()
        };
        let (mesh, _) = run(&scene(root));
        assert_eq!(mesh.face_data[0].front, Some(MaterialId(0)));
        assert_eq!(mesh.face_data[0].back, Some(MaterialId(1)));
    }

    #[test]
    fn nearest_painted_ancestor_supplies_the_material() {
        let root = Node {
            material: Some(MaterialId(0)),
            children: vec![Node {
                kind: NodeKind::Group,
                material: Some(MaterialId(1)),
                children: vec![Node {
                    kind: NodeKind::Instance,
                    faces: vec![unit_triangle(None, None)],
                    ..Node::default()
                }],
                ..Node::default()
            }],
            ..Node::default()
        };
        let (mesh, report) = run(&scene(root));
        assert_eq!(mesh.face_data[0].front, Some(MaterialId(1)));
        assert_eq!(report.materials_used, 1);
    }

    #[test]
    fn unpainted_face_without_painted_ancestors_keeps_the_default() {
        let root = Node {
            faces: vec![unit_triangle(None, None)],
            ..Node::default()
        };
        let (mesh, report) = run(&scene(root));
        assert_eq!(mesh.face_data[0].front, None);
        assert_eq!(report.default_material_faces, 1);
    }

    #[test]
    fn back_only_face_is_flagged_and_its_back_q_measured() {
        let mut face = unit_triangle(None, Some(MaterialId(0)));
        face.vertices[2].back.q = 4.0;
        let root = Node {
            faces: vec![face],
            ..Node::default()
        };
        let (mesh, report) = run(&scene(root));
        assert!(mesh.face_data[0].is_back_only());
        assert!(mesh.face_data[0].q_variance > 0.0);
        assert_eq!(report.back_only_faces, 1);
        assert_eq!(report.non_constant_q_faces, 1);
    }

    #[test]
    fn front_and_back_uvq_stay_distinct_per_corner() {
        let root = Node {
            faces: vec![unit_triangle(Some(MaterialId(0)), Some(MaterialId(1)))],
            ..Node::default()
        };
        let (mesh, _) = run(&scene(root));
        assert_eq!(mesh.corners[1].uvq, uvq(1.0, 0.0, 1.0));
        assert_eq!(mesh.corners[1].back_uvq, uvq(-1.0, 0.0, 1.0));
    }

    #[test]
    fn mirrored_placement_keeps_the_front_facing_its_normal() {
        let plain = Node {
            faces: vec![unit_triangle(None, None)],
            ..Node::default()
        };
        let mirrored = Node {
            children: vec![Node {
                kind: NodeKind::Instance,
                transform: Transform::scale(-1.0, 1.0, 1.0),
                faces: vec![unit_triangle(None, None)],
                ..Node::default()
            }],
            ..Node::default()
        };
        let (a, _) = run(&scene(plain));
        let (b, report) = run(&scene(mirrored));
        assert!(signed_z(&a, 0) > 0.0);
        assert!(signed_z(&b, 0) > 0.0);
        assert!(b.corners[0].normal.z > 0.0);
        assert_eq!(report.mirrored_nodes, 1);
    }

    #[test]
    fn zero_q_is_counted_not_propagated_into_the_maximum() {
        let mut face = unit_triangle(Some(MaterialId(0)), None);
        face.vertices[0].front.q = 0.0;
        let root = Node {
            faces: vec![face],
            ..Node::default()
        };
        let (_, report) = run(&scene(root));
        assert_eq!(report.zero_q_faces, 1);
        assert_eq!(report.max_q_variance, 0.0);
    }

    #[test]
    fn cancellation_stops_the_walk() {
        let root = Node {
            faces: vec![unit_triangle(None, None)],
            ..Node::default()
        };
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            flatten(&scene(root), &cancel, &NoProgress).unwrap_err(),
            IoError::Cancelled
        );
    }

    #[test]
    fn bounds_are_reported_in_centimetres() {
        let root = Node {
            faces: vec![unit_triangle(None, None)],
            ..Node::default()
        };
        let (_, report) = run(&scene(root));
        let (lo, hi) = report.bounds.unwrap();
        assert_eq!((hi.x - lo.x).0, 2.54);
        assert_eq!(report.triangles, 1);
    }
}
