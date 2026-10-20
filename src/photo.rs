//! What a photo says about the sun, through its perspective. With how wide the lens sees and
//! where the horizon lies in the picture (the camera's tilt; no roll), every point of the ground
//! in the photo can be put back on the ground. A shadow drawn on it then gives its real
//! direction from the camera's view, and with the object that casts it, the real height-to-length
//! ratio, so the sun's elevation, however the shadow runs toward or away from the camera.
//!
//! Distances are in camera heights (the camera stands 1 above the ground), which cancel out.

/// A photo's geometry: its size in pixels, the width of its view in degrees, and the row of
/// its horizon (the level where lines parallel to the ground meet).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub width: f64,
    pub height: f64,
    pub fov: f64,
    pub horizon: f64,
}

/// What the lines on a photo say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// Where the shadow points from the camera's view: 0° straight ahead, 90° right, ±180°
    /// toward the camera.
    pub turn: f64,
    /// The shadow's length on the ground and, when the object is drawn, its height (in camera
    /// heights), and the sun's elevation they give.
    pub length: f64,
    pub height: Option<f64>,
    pub elevation: Option<f64>,
}

impl Camera {
    /// The focal length, in pixels.
    fn focal(&self) -> f64 {
        (self.width / 2.0) / (self.fov.to_radians() / 2.0).tan()
    }

    /// How far down the camera looks, in radians (the horizon above the middle: looking down).
    fn pitch(&self) -> f64 {
        ((self.height / 2.0 - self.horizon) / self.focal()).atan()
    }

    /// The direction of the pixel's ray: right, forward (level) and down.
    fn ray(&self, [u, v]: [f64; 2]) -> [f64; 3] {
        let f = self.focal();
        let (x, y) = ((u - self.width / 2.0) / f, (v - self.height / 2.0) / f);
        let a = self.pitch();
        [x, a.cos() - y * a.sin(), y * a.cos() + a.sin()]
    }

    /// Where the pixel's ray meets the ground: right and forward of the camera. None above the
    /// horizon, where it never does.
    pub fn ground(&self, p: [f64; 2]) -> Option<[f64; 2]> {
        let [x, fwd, down] = self.ray(p);
        (down > 1e-6).then(|| [x / down, fwd / down])
    }

    /// The lines' reading: a shadow drawn from its object's foot to its tip and, maybe, the
    /// object drawn from its foot to its top (its foot is taken to be the shadow's).
    pub fn read(
        &self,
        shadow: [[f64; 2]; 2],
        object: Option<[[f64; 2]; 2]>,
    ) -> Result<Reading, &'static str> {
        const BELOW: &str = "The shadow must lie below the horizon: move the Horizon line up";
        let foot = self.ground(shadow[0]).ok_or(BELOW)?;
        let tip = self.ground(shadow[1]).ok_or(BELOW)?;
        let (dx, dz) = (tip[0] - foot[0], tip[1] - foot[1]);
        let length = dx.hypot(dz);
        if length < 1e-9 {
            return Err("The shadow is too short to measure");
        }
        let turn = dx.atan2(dz).to_degrees();
        // the top of the object: the end of its line higher in the picture
        let height = object.map(|[a, b]| {
            let top = if a[1] < b[1] { a } else { b };
            let [x, fwd, down] = self.ray(top);
            // where the top's ray is as far from the camera as the foot, it is this far down
            let reach = foot[0].hypot(foot[1]) / x.hypot(fwd);
            1.0 - down * reach
        });
        let height = height.filter(|h| *h > 0.0);
        Ok(Reading {
            turn,
            length,
            height,
            elevation: height.map(|h| h.atan2(length).to_degrees()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pixel of the world point `p` (right, forward, height above the ground; the camera 1
    /// above it), the inverse of `Camera::ray`.
    fn project(cam: &Camera, [x, z, h]: [f64; 3]) -> [f64; 2] {
        let a = cam.pitch();
        let down = 1.0 - h;
        let (yc, zc) = (down * a.cos() - z * a.sin(), down * a.sin() + z * a.cos());
        let f = cam.focal();
        [cam.width / 2.0 + f * x / zc, cam.height / 2.0 + f * yc / zc]
    }

    #[test]
    fn a_shadow_and_its_object_are_put_back_on_the_ground() {
        // a phone held a little down, horizon in the upper third
        let cam = Camera {
            width: 4032.0,
            height: 3024.0,
            fov: 70.0,
            horizon: 1100.0,
        };
        // a person 1.7 m tall, 6 m ahead and 1 m right of a camera 1.5 m up, whose 2.4 m
        // shadow points 40° right of the view
        let s = 1.5;
        let (fx, fz) = (1.0 / s, 6.0 / s);
        let turn = 40f64.to_radians();
        let (tx, tz) = (fx + 2.4 / s * turn.sin(), fz + 2.4 / s * turn.cos());
        let shadow = [project(&cam, [fx, fz, 0.0]), project(&cam, [tx, tz, 0.0])];
        let object = [
            project(&cam, [fx, fz, 0.0]),
            project(&cam, [fx, fz, 1.7 / s]),
        ];
        let r = cam.read(shadow, Some(object)).unwrap();
        assert!((r.turn - 40.0).abs() < 1e-6, "{r:?}");
        assert!((r.length - 2.4 / s).abs() < 1e-6, "{r:?}");
        assert!((r.height.unwrap() - 1.7 / s).abs() < 1e-6, "{r:?}");
        let truth = 1.7f64.atan2(2.4).to_degrees();
        assert!((r.elevation.unwrap() - truth).abs() < 1e-6, "{r:?}");
    }

    #[test]
    fn shadows_toward_the_camera_and_the_horizon_line() {
        let cam = Camera {
            width: 1000.0,
            height: 750.0,
            fov: 70.0,
            horizon: 375.0,
        };
        // a shadow falling back toward the camera and to the left
        let shadow = [
            project(&cam, [0.0, 5.0, 0.0]),
            project(&cam, [-1.0, 4.0, 0.0]),
        ];
        let r = cam.read(shadow, None).unwrap();
        assert!((r.turn + 135.0).abs() < 1e-6, "{r:?}");
        assert_eq!(r.elevation, None);
        // a shadow drawn above the horizon cannot be on the ground
        assert!(cam.read([[500.0, 300.0], [600.0, 500.0]], None).is_err());
    }
}
