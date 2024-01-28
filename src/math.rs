//implement own sin cos
use std::{ops::*, process::Output};

//Plan: Explore R3,3
//generates 6 shears, 3 pseudo-projections, 3 scales, 3 translation, 3 rotations

// Have 2 transforms
// Euclidean, for physics
// Affine, for game logic

// implement 
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Affine3 {
    pub xx: f32,
    pub yx: f32,
    pub zx: f32,
    pub _x: f32,
    
    pub xy: f32,
    pub yy: f32,
    pub zy: f32,
    pub _y: f32,

    pub xz: f32,
    pub yz: f32,
    pub zz: f32,
    pub _z: f32,
}

impl Affine3 {
    pub const IDENTITY: Self = Self {
        xx: 1.0,
        yx: 0.0,
        zx: 0.0,
        _x: 0.0,
        xy: 0.0,
        yy: 1.0,
        zy: 0.0,
        _y: 0.0,
        xz: 0.0,
        yz: 0.0,
        zz: 1.0,
        _z: 0.0,
    };

    // (A, a) * (B, b) = (A * B, a * B + b)
    pub fn compose(&self, other: &Affine3) -> Self {
        Self {
            xx: self.xx * other.xx + self.xy * other.yx + self.xz * other.zx,
            yx: self.yx * other.xx + self.yy * other.yx + self.yz * other.zx,
            zx: self.zx * other.xx + self.zy * other.yx + self.zz * other.zx,
            _x: self._x * other.xx + self._y * other.yx + self._z * other.zx + other._x,

            xy: self.xx * other.xy + self.xy * other.yy + self.xz * other.zy,
            yy: self.yx * other.xy + self.yy * other.yy + self.yz * other.zy,
            zy: self.zx * other.xy + self.zy * other.yy + self.zz * other.zy,
            _y: self._x * other.xy + self._y * other.yy + self._z * other.zy + other._y,

            xz: self.xx * other.xz + self.xy * other.yz + self.xz * other.zz,
            yz: self.yx * other.xz + self.yy * other.yz + self.yz * other.zz,
            zz: self.zx * other.xz + self.zy * other.yz + self.zz * other.zz,
            _z: self._x * other.xz + self._y * other.yz + self._z * other.zz + other._z,
        }
    }

    pub fn scale(&mut self, s: &Scale3) -> &mut Self {
        self.xx *= s.x;
        self.yx *= s.x;
        self.zx *= s.x;
        self._x *= s.x;

        self.xy *= s.y;
        self.yy *= s.y;
        self.zy *= s.y;
        self._y *= s.y;

        self.xz *= s.z;
        self.yz *= s.z;
        self.zz *= s.z;
        self._z *= s.z;
        self
    }

    pub fn translate(&mut self, v: &Vector3) -> &mut Self {
        self._x += v.x;
        self._y += v.y;
        self._z += v.z;
        self
    }

    // assumes normalized plane
    pub fn rotate(&mut self, norm: f32, b: &BiVector3) -> &mut Self {
        let zx_yz = b.zx * b.yz;
        let yz_xy = b.yz * b.xy;
        let xy_zx = b.xy * b.zx;

        let yz_yz = b.yz * b.yz;
        let zx_zx = b.zx * b.zx;
        let xy_xy = b.xy * b.xy;

        let cos = norm.cos();
        let sin = norm.sin();
        let one_sub_cos = 1.0 - cos;

        let yz_sin = b.yz * sin;
        let zx_sin = b.zx * sin;
        let xy_sin = b.xy * sin;

        let zx_yz_one_sub_cos = zx_yz * one_sub_cos;
        let yz_xy_one_sub_cos = yz_xy * one_sub_cos;
        let xy_zx_one_sub_cos = xy_zx * one_sub_cos;

        let xx = (1.0 - yz_yz) * cos + yz_yz;
        let xy = zx_yz_one_sub_cos + xy_sin;
        let xz = yz_xy_one_sub_cos - zx_sin;

        let yx = zx_yz_one_sub_cos - xy_sin;
        let yy = (1.0 - zx_zx) * cos + zx_zx;
        let yz = xy_zx_one_sub_cos + yz_sin;

        let zx = yz_xy_one_sub_cos + zx_sin;
        let zy = xy_zx_one_sub_cos - yz_sin;
        let zz = (1.0 - xy_xy) * cos + xy_xy;

        *self = Affine3 {
            xx: self.xx * xx + self.xy * yx + self.xz * zx,
            yx: self.yx * xx + self.yy * yx + self.yz * zx,
            zx: self.zx * xx + self.zy * yx + self.zz * zx,
            _x: self._x * xx + self._y * yx + self._z * zx,

            xy: self.xx * xy + self.xy * yy + self.xz * zy,
            yy: self.yx * xy + self.yy * yy + self.yz * zy,
            zy: self.zx * xy + self.zy * yy + self.zz * zy,
            _y: self._x * xy + self._y * yy + self._z * zy,

            xz: self.xx * xz + self.xy * yz + self.xz * zz,
            yz: self.yx * xz + self.yy * yz + self.yz * zz,
            zz: self.zx * xz + self.zy * yz + self.zz * zz,
            _z: self._x * xz + self._y * yz + self._z * zz,
        };

        self
    }

    // rotations are done with left to right notation
    // V x B --> V o exp(x B) = ~R * V * R where R = exp(1/2 * B)
    pub fn from(scale: Scale3, rotation: Rotor, translation: Vector3) -> Self {
        let _1zx = rotation._1 * rotation.zx;
        let _1xy = rotation._1 * rotation.xy;
        let _1yz = rotation._1 * rotation.yz;

        let zxzx = rotation.zx * rotation.zx;
        let zxxy = rotation.zx * rotation.xy;
        let xyxy = rotation.xy * rotation.xy;

        let zxyz = rotation.yz * rotation.zx;
        let yzxy = rotation.yz * rotation.xy;
        let yzyz = rotation.yz * rotation.yz;

        Self {
            xx: (1.0 - 2.0 * (zxzx + xyxy)) * scale.x,
            xy: (2.0 * (zxyz + _1xy)) * scale.x,
            xz: (2.0 * (yzxy - _1zx)) * scale.x,

            yx: (2.0 * (zxyz - _1xy)) * scale.y,
            yy: (1.0 - 2.0 * (yzyz + xyxy)) * scale.y,
            yz: (2.0 * (zxxy + _1yz)) * scale.y,

            zx: (2.0 * (yzxy + _1zx)) * scale.z,
            zy: (2.0 * (zxxy - _1yz)) * scale.z,
            zz: (1.0 - 2.0 * (yzyz + zxzx)) * scale.z,

            _x: translation.x,
            _y: translation.y,
            _z: translation.z,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BiVector3 {
    pub xy: f32,
    pub yz: f32,
    pub zx: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Scale3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Scale3 {
    pub const IDENTITY: Vector3 = Vector3 {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };

    pub fn new(x: f32, y: f32, z: f32) -> Scale3 {
        Scale3 {
            x, y, z
        }
    }
}

impl MulAssign<f32> for Scale3 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
        self.z *= rhs;
    }
}

impl Mul for Scale3 {
    type Output = Scale3;

    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
            z: self.z * rhs.z,
        }
    }
}

impl Sub for Vector3 {
    type Output = Vector3;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl Add for Vector3 {
    type Output = Vector3;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl Vector3 {
    pub const IDENTITY: Vector3 = Vector3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn norm_sqr(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn wedge(&self, rhs: &Vector3) -> BiVector3 {
        BiVector3 {
            xy: self.x * rhs.y - self.y * rhs.x,
            yz: self.y * rhs.z - self.z * rhs.y,
            zx: self.z * rhs.x - self.x * rhs.z,
        }
    }

    pub fn dot(&self, rhs: &Vector3) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub fn apply(&self, a: &Affine3) -> Self {
        Self {
            x: self.x * a.xx + self.y * a.yx + self.z * a.zx + a._x,
            y: self.x * a.xy + self.y * a.yy + self.z * a.zy + a._y,
            z: self.x * a.xz + self.y * a.yz + self.z * a.zz + a._z,
        }
    }
}

impl Div<f32> for Vector3 {
    type Output = Vector3;

    fn div(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

impl Mul<f32> for Vector3 {
    type Output = Vector3;

    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl SubAssign for Vector3 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}


impl AddAssign for Vector3 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl Neg for Vector3 {
    type Output = Vector3;

    fn neg(self) -> Self::Output {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}

impl BiVector3 {
    pub const fn new(xy: f32, yz: f32, zx: f32) -> Self {
        Self { xy, yz, zx }
    }

    pub fn commute(&self, other: &BiVector3) -> BiVector3 {
        BiVector3 {
            xy: self.yz * other.zx - other.yz * self.zx,
            yz: self.zx * other.xy - other.zx * self.xy,
            zx: self.xy * other.yz - other.xy * self.yz,
        }
    }

    pub fn norm_sqr(&self) -> f32 {
        self.xy * self.xy + self.yz * self.yz + self.zx * self.zx
    }

    /// In R3 the biVector squares to a negative scalar
    /// hence we can factor the BiVector to a scalar and unit biVector
    /// and employ Taylor expansion from there without worrying about non-commuting biVectors
    pub fn exp(mut self) -> Rotor {
        let norm_sqr = self.norm_sqr();
        if norm_sqr == 0.0 {
            return Rotor::IDENTITY;
        }
        let norm = norm_sqr.sqrt();
        let cos = norm.cos();
        self = (self / norm) * norm.sin();

        Rotor {
            _1: cos,
            xy: self.xy,
            yz: self.yz,
            zx: self.zx,
        }
    }
}

impl AddAssign for BiVector3 {
    fn add_assign(&mut self, rhs: Self) {
        self.xy += rhs.xy;
        self.yz += rhs.yz;
        self.zx += rhs.zx;
    }
}

impl Mul<f32> for BiVector3 {
    type Output = BiVector3;

    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            xy: self.xy * rhs,
            yz: self.yz * rhs,
            zx: self.zx * rhs,
        }
    }
}

impl Div<f32> for BiVector3 {
    type Output = BiVector3;

    fn div(self, rhs: f32) -> Self::Output {
        Self {
            xy: self.xy / rhs,
            yz: self.yz / rhs,
            zx: self.zx / rhs,
        }
    }
}

impl Mul<Rotor> for BiVector3 {
    type Output = Rotor;

    fn mul(self, rhs: Rotor) -> Self::Output {
        Rotor {
            _1:-self.xy * rhs.xy - self.yz * rhs.yz - self.zx * rhs.zx,
            xy: self.xy * rhs._1 + self.yz * rhs.zx - self.zx * rhs.yz,
            yz:-self.xy * rhs.zx + self.yz * rhs._1 + self.zx * rhs.xy,
            zx: self.yz * rhs.xy - self.xy * rhs.yz + self.zx * rhs._1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rotor {
    _1: f32,
    xy: f32,
    yz: f32,
    zx: f32,
}

impl Rotor {
    pub const IDENTITY: Rotor = Rotor {
        _1: 1.0,
        xy: 0.0,
        yz: 0.0,
        zx: 0.0,
    };

    pub fn norm_sqr(&self) -> f32 {
        self._1 * self._1 + self.xy * self.xy + self.yz * self.yz + self.zx * self.zx
    }
}

impl Mul for Rotor {
    type Output = Rotor;

    // not sure if the signs are right hehehe... ;/
    fn mul(self, rhs: Self) -> Self::Output {
        Rotor {
            _1: self._1 * rhs._1 - self.xy * rhs.xy - self.yz * rhs.yz - self.zx * rhs.zx,
            xy: self._1 * rhs.xy + self.xy * rhs._1 + self.yz * rhs.zx - self.zx * rhs.yz,
            yz: self._1 * rhs.yz - self.xy * rhs.zx + self.yz * rhs._1 + self.zx * rhs.xy,
            zx: self._1 * rhs.zx + self.yz * rhs.xy - self.xy * rhs.yz + self.zx * rhs._1,
        }
    }
}

impl DivAssign<f32> for Rotor {
    /// should only be used to normalise a rotor
    fn div_assign(&mut self, rhs: f32) {
        self._1 /= rhs;
        self.xy /= rhs;
        self.yz /= rhs;
        self.zx /= rhs;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable, PartialEq)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    pub const NAN: Self = Vector2{ x: f32::NAN, y: f32::NAN };
    pub const IDENTITY: Self = Vector2{ x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x, y
        }
    }

    #[inline]
    pub fn wedge(self, rhs: Vector2) -> BiVector2 {
        BiVector2 {
            xy: self.x * rhs.y - self.y * rhs.x,
        }
    }
}

impl Mul<f32> for Vector2 {
    type Output = Vector2;

    fn mul(self, rhs: f32) -> Self::Output {
        Vector2 {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Add for Vector2 {
    type Output = Vector2;

    fn add(self, rhs: Self) -> Self::Output {
        Vector2 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

pub struct BiVector2 {
    pub xy: f32,
}


impl Sub for Vector2 {
    type Output = Vector2;

    fn sub(self, rhs: Self) -> Self::Output {
        Vector2 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Scale2 {
    pub x: f32,
    pub y: f32,    
}

impl Scale2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x, y
        }
    }
}

impl Neg for Vector2 {
    type Output = Vector2;

    fn neg(self) -> Self::Output {
        Vector2 {
            x: -self.x,
            y: -self.y,
        }
    }
}