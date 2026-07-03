use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::error::Error;
use std::fmt;

use openbg_domain::{GridPoint, NavigationGrid};

const CARDINAL_COST: u32 = 10;
const DIAGONAL_COST: u32 = 14;

/// Finds a deterministic eight-direction path without cutting blocked corners.
///
/// The returned path includes both `start` and `goal`.
///
/// # Errors
///
/// Returns [`PathError`] when an endpoint is outside the grid, blocked, or no
/// connected route exists.
pub fn find_path(
    grid: &NavigationGrid,
    start: GridPoint,
    goal: GridPoint,
) -> Result<Vec<GridPoint>, PathError> {
    validate_endpoint(grid, start, true)?;
    validate_endpoint(grid, goal, false)?;
    if start == goal {
        return Ok(vec![start]);
    }

    let cell_count = usize::from(grid.width()) * usize::from(grid.height());
    let mut costs = vec![u32::MAX; cell_count];
    let mut previous = vec![None; cell_count];
    let mut closed = vec![false; cell_count];
    let start_index = grid.index(start).ok_or(PathError::StartOutside)?;
    costs[start_index] = 0;

    // Reverse gives a min-heap. Remaining tuple fields provide stable tie breaks.
    let mut open = BinaryHeap::new();
    let initial_h = heuristic(start, goal);
    open.push(Reverse((initial_h, initial_h, start.y, start.x)));

    while let Some(Reverse((_, _, y, x))) = open.pop() {
        let current = GridPoint::new(x, y);
        let current_index = grid.index(current).ok_or(PathError::NoPath)?;
        if closed[current_index] {
            continue;
        }
        if current == goal {
            return Ok(reconstruct(grid, &previous, start, goal));
        }
        closed[current_index] = true;

        for (neighbor, step_cost) in neighbors(grid, current) {
            let neighbor_index = grid.index(neighbor).ok_or(PathError::NoPath)?;
            if closed[neighbor_index] {
                continue;
            }
            let candidate = costs[current_index].saturating_add(step_cost);
            if candidate >= costs[neighbor_index] {
                continue;
            }
            costs[neighbor_index] = candidate;
            previous[neighbor_index] = Some(current);
            let h = heuristic(neighbor, goal);
            open.push(Reverse((
                candidate.saturating_add(h),
                h,
                neighbor.y,
                neighbor.x,
            )));
        }
    }
    Err(PathError::NoPath)
}

fn validate_endpoint(
    grid: &NavigationGrid,
    point: GridPoint,
    start: bool,
) -> Result<(), PathError> {
    if !grid.contains(point) {
        return Err(if start {
            PathError::StartOutside
        } else {
            PathError::GoalOutside
        });
    }
    if !grid.is_walkable(point) {
        return Err(if start {
            PathError::StartBlocked
        } else {
            PathError::GoalBlocked
        });
    }
    Ok(())
}

fn heuristic(from: GridPoint, to: GridPoint) -> u32 {
    let dx = u32::from(from.x.abs_diff(to.x));
    let dy = u32::from(from.y.abs_diff(to.y));
    let diagonal = dx.min(dy);
    let straight = dx.max(dy) - diagonal;
    diagonal * DIAGONAL_COST + straight * CARDINAL_COST
}

fn neighbors(grid: &NavigationGrid, point: GridPoint) -> Vec<(GridPoint, u32)> {
    const DIRECTIONS: [(i16, i16); 8] = [
        (0, -1),
        (-1, 0),
        (1, 0),
        (0, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
        (1, 1),
    ];
    let mut result = Vec::with_capacity(8);
    for (dx, dy) in DIRECTIONS {
        let Some(x) = point.x.checked_add_signed(dx) else {
            continue;
        };
        let Some(y) = point.y.checked_add_signed(dy) else {
            continue;
        };
        let neighbor = GridPoint::new(x, y);
        if !grid.is_walkable(neighbor) {
            continue;
        }
        let diagonal = dx != 0 && dy != 0;
        if diagonal {
            let side_x = GridPoint::new(x, point.y);
            let side_y = GridPoint::new(point.x, y);
            if !grid.is_walkable(side_x) || !grid.is_walkable(side_y) {
                continue;
            }
        }
        result.push((
            neighbor,
            if diagonal {
                DIAGONAL_COST
            } else {
                CARDINAL_COST
            },
        ));
    }
    result
}

fn reconstruct(
    grid: &NavigationGrid,
    previous: &[Option<GridPoint>],
    start: GridPoint,
    goal: GridPoint,
) -> Vec<GridPoint> {
    let mut path = vec![goal];
    let mut current = goal;
    while current != start {
        let index = grid
            .index(current)
            .expect("reconstructed points originated inside the grid");
        current = previous[index].expect("a reached goal has a predecessor chain");
        path.push(current);
    }
    path.reverse();
    path
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathError {
    StartOutside,
    GoalOutside,
    StartBlocked,
    GoalBlocked,
    NoPath,
}

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StartOutside => "path start is outside the navigation grid",
            Self::GoalOutside => "path goal is outside the navigation grid",
            Self::StartBlocked => "path start is blocked",
            Self::GoalBlocked => "path goal is blocked",
            Self::NoPath => "no path connects the endpoints",
        })
    }
}

impl Error for PathError {}

#[cfg(test)]
mod tests {
    use openbg_domain::{GridPoint, NavigationGrid};

    use super::{find_path, PathError};

    #[test]
    fn routes_through_the_only_wall_gap_deterministically() {
        let mut cells = vec![1_u8; 25];
        for y in [0_usize, 1, 3, 4] {
            cells[y * 5 + 2] = 0;
        }
        let grid = NavigationGrid::new(5, 5, cells).expect("valid grid");
        let expected = vec![
            GridPoint::new(0, 0),
            GridPoint::new(1, 1),
            GridPoint::new(1, 2),
            GridPoint::new(2, 2),
            GridPoint::new(3, 2),
            GridPoint::new(4, 3),
            GridPoint::new(4, 4),
        ];
        assert_eq!(
            find_path(&grid, GridPoint::new(0, 0), GridPoint::new(4, 4)),
            Ok(expected.clone())
        );
        assert_eq!(
            find_path(&grid, GridPoint::new(0, 0), GridPoint::new(4, 4)),
            Ok(expected)
        );
    }

    #[test]
    fn does_not_cut_a_blocked_diagonal_corner() {
        let grid = NavigationGrid::new(2, 2, vec![1, 0, 0, 1]).expect("valid grid");
        assert_eq!(
            find_path(&grid, GridPoint::new(0, 0), GridPoint::new(1, 1)),
            Err(PathError::NoPath)
        );
    }
}
