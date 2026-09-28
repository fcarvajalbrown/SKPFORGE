use crate::scene::{LoadStatus, ModelUnits};
use skp_core::mesh::Point;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportReport {
    pub load_status: LoadStatus,
    pub units: ModelUnits,
    pub faces: usize,
    pub triangles: usize,
    pub groups: usize,
    pub instances: usize,
    pub mirrored_nodes: usize,
    pub hidden_skipped: usize,
    pub materials_in_model: usize,
    pub materials_used: usize,
    pub default_material_faces: usize,
    pub back_only_faces: usize,
    pub non_constant_q_faces: usize,
    pub zero_q_faces: usize,
    pub max_q_variance: f64,
    pub bounds: Option<(Point, Point)>,
}

impl fmt::Display for LoadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadStatus::Current => write!(f, "current"),
            LoadStatus::NewerThanSdk => write!(f, "saved by a newer SketchUp than the SDK"),
        }
    }
}

impl fmt::Display for ModelUnits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            ModelUnits::Inches => "inches",
            ModelUnits::Feet => "feet",
            ModelUnits::Millimetres => "millimetres",
            ModelUnits::Centimetres => "centimetres",
            ModelUnits::Metres => "metres",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for ImportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "load status            {}", self.load_status)?;
        writeln!(f, "display units          {}", self.units)?;
        writeln!(f, "faces                  {}", self.faces)?;
        writeln!(f, "triangles              {}", self.triangles)?;
        writeln!(f, "groups                 {}", self.groups)?;
        writeln!(f, "component instances    {}", self.instances)?;
        writeln!(f, "mirrored placements    {}", self.mirrored_nodes)?;
        writeln!(f, "hidden, skipped        {}", self.hidden_skipped)?;
        writeln!(f, "materials in model     {}", self.materials_in_model)?;
        writeln!(f, "materials used         {}", self.materials_used)?;
        writeln!(f, "default-material faces {}", self.default_material_faces)?;
        writeln!(f, "back-only faces        {}", self.back_only_faces)?;
        writeln!(f, "faces with varying q   {}", self.non_constant_q_faces)?;
        writeln!(f, "faces with q of zero   {}", self.zero_q_faces)?;
        writeln!(f, "max q variance         {:e}", self.max_q_variance)?;
        match self.bounds {
            Some((min, max)) => write!(
                f,
                "bounds (cm)            {:.3} x {:.3} x {:.3}",
                (max.x - min.x).0,
                (max.y - min.y).0,
                (max.z - min.z).0
            ),
            None => write!(f, "bounds (cm)            empty"),
        }
    }
}
