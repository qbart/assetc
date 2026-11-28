//! The vectors inside the crate: matma's, with the few helpers it lacks.

pub(crate) type Vec3 = matma::float3;
pub(crate) type DVec3 = matma::double3;

pub(crate) trait Ext {
    type Scalar;
    /// The largest component.
    fn max_element(self) -> Self::Scalar;
    /// The index of the largest component, the first of equal ones.
    fn max_position(self) -> usize;
}

macro_rules! ext {
    ($t:ty) => {
        impl Ext for matma::Vector3<$t> {
            type Scalar = $t;
            fn max_element(self) -> $t {
                let max = |a: $t, b: $t| if a > b { a } else { b };
                max(self.x, max(self.y, self.z))
            }
            fn max_position(self) -> usize {
                let mut max = self.x;
                let mut index = 0;
                if self.y > max {
                    max = self.y;
                    index = 1;
                }
                if self.z > max {
                    index = 2;
                }
                index
            }
        }
    };
}
ext!(f32);
ext!(f64);

pub(crate) trait AsDouble {
    fn to_dvec3(self) -> DVec3;
}

impl AsDouble for Vec3 {
    fn to_dvec3(self) -> DVec3 {
        self.map(|c| c as f64)
    }
}
