use geometry_core::Point;

#[derive(Debug, Clone)]
pub enum PathVerb {
    MoveTo(Point),
    LineTo(Point),
    QuadTo {
        ctrl: Point,
        to: Point,
    },
    CubicTo {
        ctrl1: Point,
        ctrl2: Point,
        to: Point,
    },
    Close,
}

#[derive(Debug, Clone, Default)]
pub struct PathData {
    pub(crate) verbs: Vec<PathVerb>,
}

impl PathData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn verbs(&self) -> &[PathVerb] {
        &self.verbs
    }

    pub fn move_to(mut self, p: Point) -> Self {
        self.verbs.push(PathVerb::MoveTo(p));
        self
    }

    pub fn line_to(mut self, p: Point) -> Self {
        self.verbs.push(PathVerb::LineTo(p));
        self
    }

    pub fn quad_to(mut self, ctrl: Point, to: Point) -> Self {
        self.verbs.push(PathVerb::QuadTo { ctrl, to });
        self
    }

    pub fn cubic_to(mut self, ctrl1: Point, ctrl2: Point, to: Point) -> Self {
        self.verbs.push(PathVerb::CubicTo { ctrl1, ctrl2, to });
        self
    }

    pub fn close(mut self) -> Self {
        self.verbs.push(PathVerb::Close);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_data_new_is_empty() {
        let path = PathData::new();
        assert!(path.verbs().is_empty());
    }

    #[test]
    fn path_data_move_to_adds_verb() {
        let path = PathData::new().move_to(Point::new(0.0, 0.0));
        assert_eq!(path.verbs().len(), 1);
        assert!(matches!(path.verbs()[0], PathVerb::MoveTo(_)));
    }

    #[test]
    fn path_data_line_to_adds_verb() {
        let path = PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(1.0, 1.0));
        assert_eq!(path.verbs().len(), 2);
        assert!(matches!(path.verbs()[1], PathVerb::LineTo(_)));
    }

    #[test]
    fn path_data_close_adds_verb() {
        let path = PathData::new().move_to(Point::new(0.0, 0.0)).close();
        assert!(matches!(path.verbs().last().unwrap(), PathVerb::Close));
    }

    #[test]
    fn path_data_quad_to_adds_verb() {
        let path = PathData::new().quad_to(Point::new(1.0, 0.0), Point::new(2.0, 0.0));
        assert!(matches!(path.verbs()[0], PathVerb::QuadTo { .. }));
    }

    #[test]
    fn path_data_cubic_to_adds_verb() {
        let path = PathData::new().cubic_to(
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(3.0, 0.0),
        );
        assert!(matches!(path.verbs()[0], PathVerb::CubicTo { .. }));
    }

    #[test]
    fn path_data_builder_accumulates_verbs() {
        let path = PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(1.0, 0.0))
            .line_to(Point::new(1.0, 1.0))
            .close();
        assert_eq!(path.verbs().len(), 4);
    }
}
