//! Where a place is drawn on a map that may be tilted. A flat map places it
//! with [`Frame`]'s own arithmetic; a `vector-*` map that gpui-maps has
//! tilted (`vector::tilt`) places it with gpui-maps' camera, so its pins and
//! routes stand on the streets the perspective draws.

use super::view::Frame;

#[cfg(feature = "map-vector")]
pub(super) type Camera = gpui_maps::render::Camera;
/// No camera without `map-vector`: every map is flat.
#[cfg(not(feature = "map-vector"))]
#[derive(Clone, Copy, Debug)]
pub(super) enum Camera {}

/// gpui-maps' camera, while it tilts the map; `None` on a flat map.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Tilt(pub(super) Option<Camera>);

impl Tilt {
    /// Where `(lat, lon)` is drawn in the box; `None` when tilted past the
    /// eye (behind it, or too near to draw).
    pub(super) fn at(self, frame: &Frame, lat: f64, lon: f64) -> Option<(f32, f32)> {
        match self.0 {
            None => Some(frame.at(lat, lon)),
            #[cfg(feature = "map-vector")]
            Some(camera) => camera
                .project_lon_lat_alt(lon, lat, 0.0)
                .map(|(x, y, _)| (x, y)),
            #[cfg(not(feature = "map-vector"))]
            Some(camera) => match camera {},
        }
    }

    /// A route's corners in the box. Tilted, the corners past the eye are
    /// left out.
    pub(super) fn route(self, frame: &Frame, places: &[[f64; 2]]) -> Vec<(f32, f32)> {
        match self.0 {
            None => frame.route(places),
            Some(_) => places
                .iter()
                .filter_map(|&[lat, lon]| self.at(frame, lat, lon))
                .collect(),
        }
    }

    /// [`Frame::copies`]; a tilted map shows no world twice.
    pub(super) fn copies(
        self,
        frame: &Frame,
        low: (f32, f32),
        high: (f32, f32),
        padding: f32,
    ) -> std::ops::RangeInclusive<i64> {
        match self.0 {
            None => frame.copies(low, high, padding),
            Some(_) => 0..=0,
        }
    }
}

#[cfg(all(test, feature = "map-vector"))]
#[path = "tilt_tests.rs"]
mod tests;
