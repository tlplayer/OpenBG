use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GridPoint {
    pub x: u16,
    pub y: u16,
}

impl GridPoint {
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

/// Canonical Infinity search-map cells in top-left row-major order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavigationGrid {
    width: u16,
    height: u16,
    flags: Vec<u8>,
}

impl NavigationGrid {
    /// Creates a validated navigation grid while preserving original cell flags.
    ///
    /// # Errors
    ///
    /// Returns [`GridError`] when either dimension is zero or the cell count
    /// does not equal `width * height`.
    pub fn new(width: u16, height: u16, flags: Vec<u8>) -> Result<Self, GridError> {
        if width == 0 || height == 0 {
            return Err(GridError::Empty);
        }
        let expected = usize::from(width) * usize::from(height);
        if flags.len() != expected {
            return Err(GridError::WrongCellCount {
                expected,
                actual: flags.len(),
            });
        }
        Ok(Self {
            width,
            height,
            flags,
        })
    }

    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    #[must_use]
    pub fn contains(&self, point: GridPoint) -> bool {
        point.x < self.width && point.y < self.height
    }

    #[must_use]
    pub fn flags(&self, point: GridPoint) -> Option<u8> {
        self.index(point).map(|index| self.flags[index])
    }

    #[must_use]
    pub fn is_walkable(&self, point: GridPoint) -> bool {
        // Infinity search-map pixels are terrain classes, not bit flags.
        // 0/8/10/12/13 are obstacles of different visibility/flyability;
        // 14 is a world-map exit and is not itself walkable.
        self.flags(point)
            .is_some_and(|terrain| matches!(terrain, 1..=7 | 9 | 11 | 15))
    }

    #[must_use]
    pub fn cells(&self) -> &[u8] {
        &self.flags
    }

    #[must_use]
    pub fn index(&self, point: GridPoint) -> Option<usize> {
        self.contains(point)
            .then(|| usize::from(point.y) * usize::from(self.width) + usize::from(point.x))
    }

    #[must_use]
    pub fn point(&self, index: usize) -> Option<GridPoint> {
        (index < self.flags.len()).then(|| GridPoint {
            x: u16::try_from(index % usize::from(self.width))
                .expect("grid X is bounded by u16 width"),
            y: u16::try_from(index / usize::from(self.width))
                .expect("grid Y is bounded by u16 height"),
        })
    }

    /// Finds the closest walkable cell using deterministic expanding squares.
    #[must_use]
    pub fn nearest_walkable(&self, origin: GridPoint, maximum_radius: u16) -> Option<GridPoint> {
        if self.is_walkable(origin) {
            return Some(origin);
        }
        for radius in 1..=maximum_radius {
            let min_x = origin.x.saturating_sub(radius);
            let max_x = origin.x.saturating_add(radius).min(self.width - 1);
            let min_y = origin.y.saturating_sub(radius);
            let max_y = origin.y.saturating_add(radius).min(self.height - 1);
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    if x != min_x && x != max_x && y != min_y && y != max_y {
                        continue;
                    }
                    let point = GridPoint::new(x, y);
                    if self.is_walkable(point) {
                        return Some(point);
                    }
                }
            }
        }
        None
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GridError {
    Empty,
    WrongCellCount { expected: usize, actual: usize },
}

impl fmt::Display for GridError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("navigation grid dimensions must be non-zero"),
            Self::WrongCellCount { expected, actual } => {
                write!(
                    formatter,
                    "navigation grid needs {expected} cells, got {actual}"
                )
            }
        }
    }
}

impl Error for GridError {}

#[cfg(test)]
mod tests {
    use super::{GridPoint, NavigationGrid};

    #[test]
    fn nearest_walkable_has_stable_top_left_tie_breaking() {
        let grid = NavigationGrid::new(3, 3, vec![1, 0, 1, 0, 0, 0, 1, 0, 1]).expect("valid grid");
        assert_eq!(
            grid.nearest_walkable(GridPoint::new(1, 1), 1),
            Some(GridPoint::new(0, 0))
        );
    }

    #[test]
    fn uses_infinity_terrain_classes_instead_of_bit_flags() {
        let grid = NavigationGrid::new(16, 1, (0_u8..=15).collect()).expect("valid grid");
        let walkable = (0_u16..16)
            .filter(|x| grid.is_walkable(GridPoint::new(*x, 0)))
            .collect::<Vec<_>>();
        assert_eq!(walkable, vec![1, 2, 3, 4, 5, 6, 7, 9, 11, 15]);
        assert!(
            !grid.is_walkable(GridPoint::new(10, 0)),
            "walls block walking"
        );
        assert!(
            !grid.is_walkable(GridPoint::new(14, 0)),
            "world exits are not floor"
        );
    }
}
