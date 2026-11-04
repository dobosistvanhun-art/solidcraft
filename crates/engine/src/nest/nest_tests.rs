//! The occupancy grid and part masks: bits found where they are, and two masks that don't meet
//! keep their outlines the spacing apart.

use solidcraft_geom::Vec2;

use super::raster::{self, Grid};

#[test]
fn the_grid_finds_the_last_taken_cell_in_a_span() {
    let mut g = Grid::new(200, 3);
    g.set_span(1, 10, 20);
    g.set_span(1, 130, 131);
    assert_eq!(g.last_set(1, 0, 200), Some(130));
    assert_eq!(g.last_set(1, 0, 130), Some(19));
    assert_eq!(g.last_set(1, 20, 130), None);
    assert_eq!(g.last_set(0, 0, 200), None);
    assert_eq!(g.last_set(1, 64, 128), None);
    assert_eq!(g.taken(), 11);
}

/// A 10 mm square at reach 1 takes the cells within 1 mm of it: on 1 mm cells, 12 or 13 across.
#[test]
fn a_mask_takes_the_cells_within_reach_of_the_outline() {
    let sq = vec![Vec2::new(-5.0, -5.0), Vec2::new(5.0, -5.0), Vec2::new(5.0, 5.0), Vec2::new(-5.0, 5.0)];
    let m = raster::mask(&[sq], 1.0, 1.0, Vec2::ZERO).expect("a mask");
    let widest = m.rows.iter().map(|r| r.iter().map(|(a, b)| b - a).sum::<usize>()).max().unwrap_or(0);
    assert!((12..=14).contains(&widest), "{widest}");
    let mut budget = u64::MAX;
    let mut g = Grid::new(100, 100);
    let at = raster::find(&g, &m, &mut budget).expect("room");
    assert_eq!(at, (0, 0));
    raster::put(&mut g, &m, 0, 0);
    let next = raster::find(&g, &m, &mut budget).expect("room");
    assert!(next.0 >= m.w - 2 && next.1 == 0, "beside the first: {next:?}");
}
