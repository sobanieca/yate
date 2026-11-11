//! The panes of the editor: a tree of groups that lay their tiles out side
//! by side or one above the other.
//!
//! The tiles of a group share its space evenly; `Group::extra` holds the
//! columns or rows a resize moved between two neighbors. Splitting a pane
//! in the direction of its group adds a tile to the group, so the group
//! stays even; in the other direction the pane becomes a group of two.
//! Groups of the same direction never nest.
//!
//! The cursor and scroll of the focused pane live in its document; the
//! other panes keep their own in `Pane`.

use crate::editor::document::ViewState;
use crate::editor::selection::Selection;
use crate::ui::surface::Rect;

pub const MIN_PANE_WIDTH: u16 = 10;
/// The title row and two lines of text.
pub const MIN_PANE_HEIGHT: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    SideBySide,
    Stacked,
}

impl Direction {
    fn length(self, area: Rect) -> u16 {
        match self {
            Direction::SideBySide => area.w,
            Direction::Stacked => area.h,
        }
    }

    /// Side by side tiles have a column between them; the title row of a
    /// pane separates stacked tiles.
    fn gap(self) -> u16 {
        match self {
            Direction::SideBySide => 1,
            Direction::Stacked => 0,
        }
    }
}

#[derive(Clone, Default)]
pub struct Pane {
    pub id: usize,
    pub doc_id: usize,
    pub selection: Selection,
    pub view: ViewState,
}

#[derive(Clone)]
pub enum Tile {
    Pane(Pane),
    Group(Group),
}

#[derive(Clone)]
pub struct Group {
    pub direction: Direction,
    pub tiles: Vec<Tile>,
    /// What each tile has more (or less) than its even share.
    extra: Vec<i32>,
}

/// Where a pane is drawn: the row with its file name, and its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneArea {
    pub id: usize,
    pub title: Rect,
    pub body: Rect,
}

impl PaneArea {
    pub fn size(&self) -> (u16, u16) {
        (self.body.w, self.title.h + self.body.h)
    }

    fn whole(&self) -> Rect {
        let (width, height) = self.size();
        Rect::new(self.title.x, self.title.y, width, height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Above,
    Below,
}

/// The pane that touches pane `from` on `side`. When several do, the one
/// level with the screen cell `near` (the cursor).
pub fn neighbor(panes: &[PaneArea], from: usize, side: Side, near: (u16, u16)) -> Option<usize> {
    let own = panes.iter().find(|pane| pane.id == from)?.whole();
    let gap = |area: &Rect| match side {
        Side::Left => own.x.checked_sub(area.right()),
        Side::Right => area.x.checked_sub(own.right()),
        Side::Above => own.y.checked_sub(area.bottom()),
        Side::Below => area.y.checked_sub(own.bottom()),
    };
    let off_level = |area: &Rect| match side {
        Side::Left | Side::Right => outside(near.1, area.y, area.bottom()),
        Side::Above | Side::Below => outside(near.0, area.x, area.right()),
    };
    panes
        .iter()
        .filter(|pane| pane.id != from)
        .filter_map(|pane| {
            let area = pane.whole();
            Some(((gap(&area)?, off_level(&area)), pane.id))
        })
        .min()
        .map(|(_, id)| id)
}

/// How far `value` is outside `start..end`.
fn outside(value: u16, start: u16, end: u16) -> u16 {
    if value < start {
        start - value
    } else {
        (value + 1).saturating_sub(end)
    }
}

#[derive(Default)]
pub struct Placement {
    pub panes: Vec<PaneArea>,
    /// The columns between side by side tiles.
    pub separators: Vec<Rect>,
}

#[derive(Clone)]
pub struct Panes {
    root: Tile,
    pub focused: usize,
    pub sync_scroll: bool,
    /// Document id and scroll of the focused pane at the previous frame.
    pub(super) last_scroll: Option<(usize, usize, usize)>,
    next_id: usize,
}

impl Default for Panes {
    fn default() -> Self {
        Panes {
            root: Tile::Pane(Pane::default()),
            focused: 0,
            sync_scroll: false,
            last_scroll: None,
            next_id: 1,
        }
    }
}

impl Group {
    fn even_sizes(&self, length: u16) -> Vec<u16> {
        let count = self.tiles.len().max(1);
        let gaps = self.direction.gap() * (count as u16 - 1);
        let available = length.saturating_sub(gaps) as usize;
        let larger_from = count - available % count;
        (0..count)
            .map(|index| (available / count + usize::from(index >= larger_from)) as u16)
            .collect()
    }

    /// The length of each tile: the even shares with the resizes, or the
    /// even shares alone when the resizes no longer fit.
    fn sizes(&self, length: u16) -> Vec<u16> {
        let even = self.even_sizes(length);
        let resized: Vec<i32> = even
            .iter()
            .zip(&self.extra)
            .map(|(size, extra)| *size as i32 + extra)
            .collect();
        let fits = resized
            .iter()
            .zip(&self.tiles)
            .all(|(size, tile)| *size >= tile.min_size(self.direction) as i32);
        if fits && resized.len() == even.len() {
            resized.into_iter().map(|size| size as u16).collect()
        } else {
            even
        }
    }

    fn areas(&self, area: Rect) -> Vec<Rect> {
        let mut offset = 0;
        self.sizes(self.direction.length(area))
            .into_iter()
            .map(|size| {
                let tile = match self.direction {
                    Direction::SideBySide => Rect::new(area.x + offset, area.y, size, area.h),
                    Direction::Stacked => Rect::new(area.x, area.y + offset, area.w, size),
                };
                offset += size + self.direction.gap();
                tile
            })
            .collect()
    }

    fn reset_sizes(&mut self) {
        self.extra = vec![0; self.tiles.len()];
    }

    /// Moves up to `amount` columns or rows from the tile next to `index`
    /// to it (or back, when negative). Returns what was moved.
    fn grow(&mut self, index: usize, amount: i32, length: u16) -> i32 {
        let neighbor = if index + 1 < self.tiles.len() {
            index + 1
        } else {
            index - 1
        };
        let even = self.even_sizes(length);
        let mut sizes: Vec<i32> = self
            .sizes(length)
            .into_iter()
            .map(|size| size as i32)
            .collect();
        let room =
            |tile: usize| (sizes[tile] - self.tiles[tile].min_size(self.direction) as i32).max(0);
        let moved = amount.clamp(-room(index), room(neighbor));
        sizes[index] += moved;
        sizes[neighbor] -= moved;
        self.extra = sizes
            .iter()
            .zip(&even)
            .map(|(size, even)| size - *even as i32)
            .collect();
        moved
    }
}

impl Tile {
    fn contains(&self, id: usize) -> bool {
        match self {
            Tile::Pane(pane) => pane.id == id,
            Tile::Group(group) => group.tiles.iter().any(|tile| tile.contains(id)),
        }
    }

    fn is_pane(&self, id: usize) -> bool {
        matches!(self, Tile::Pane(pane) if pane.id == id)
    }

    fn collect<'a>(&'a self, panes: &mut Vec<&'a Pane>) {
        match self {
            Tile::Pane(pane) => panes.push(pane),
            Tile::Group(group) => group.tiles.iter().for_each(|tile| tile.collect(panes)),
        }
    }

    fn collect_mut<'a>(&'a mut self, panes: &mut Vec<&'a mut Pane>) {
        match self {
            Tile::Pane(pane) => panes.push(pane),
            Tile::Group(group) => group
                .tiles
                .iter_mut()
                .for_each(|tile| tile.collect_mut(panes)),
        }
    }

    fn first_pane(&self) -> usize {
        match self {
            Tile::Pane(pane) => pane.id,
            Tile::Group(group) => group.tiles.first().map_or(0, Tile::first_pane),
        }
    }

    fn last_pane(&self) -> usize {
        match self {
            Tile::Pane(pane) => pane.id,
            Tile::Group(group) => group.tiles.last().map_or(0, Tile::last_pane),
        }
    }

    fn min_size(&self, direction: Direction) -> u16 {
        match self {
            Tile::Pane(_) => match direction {
                Direction::SideBySide => MIN_PANE_WIDTH,
                Direction::Stacked => MIN_PANE_HEIGHT,
            },
            Tile::Group(group) => {
                let sizes = group.tiles.iter().map(|tile| tile.min_size(direction));
                if group.direction == direction {
                    let gaps = direction.gap() * (group.tiles.len() as u16).saturating_sub(1);
                    sizes.sum::<u16>() + gaps
                } else {
                    sizes.max().unwrap_or(0)
                }
            }
        }
    }

    /// Puts `pane` after pane `target`. Gives the pane back when `target`
    /// is not in this tile.
    fn insert_after(&mut self, target: usize, direction: Direction, pane: Pane) -> Option<Pane> {
        match self {
            Tile::Pane(existing) if existing.id == target => {
                let group = Tile::Group(Group {
                    direction,
                    tiles: Vec::new(),
                    extra: vec![0, 0],
                });
                let existing = std::mem::replace(self, group);
                if let Tile::Group(group) = self {
                    group.tiles = vec![existing, Tile::Pane(pane)];
                }
                None
            }
            Tile::Pane(_) => Some(pane),
            Tile::Group(group) => {
                let sibling = group.tiles.iter().position(|tile| tile.is_pane(target));
                if let Some(index) = sibling
                    && group.direction == direction
                {
                    group.tiles.insert(index + 1, Tile::Pane(pane));
                    group.reset_sizes();
                    return None;
                }
                let mut pane = pane;
                for tile in &mut group.tiles {
                    pane = tile.insert_after(target, direction, pane)?;
                }
                Some(pane)
            }
        }
    }

    /// Takes pane `id` out of the groups of this tile. Returns it with the
    /// pane next to it.
    fn remove(&mut self, id: usize) -> Option<(Pane, usize)> {
        let Tile::Group(group) = self else {
            return None;
        };
        let removed = match group.tiles.iter().position(|tile| tile.is_pane(id)) {
            Some(index) => {
                let Tile::Pane(pane) = group.tiles.remove(index) else {
                    return None;
                };
                group.reset_sizes();
                let neighbor = match index {
                    0 => group.tiles[0].first_pane(),
                    _ => group.tiles[index - 1].last_pane(),
                };
                Some((pane, neighbor))
            }
            None => group.tiles.iter_mut().find_map(|tile| tile.remove(id)),
        };
        if removed.is_some() {
            self.simplify();
        }
        removed
    }

    /// A group of one tile becomes that tile, and a group inside a group of
    /// the same direction gives its tiles to it.
    fn simplify(&mut self) {
        let Tile::Group(group) = self else {
            return;
        };
        if group.tiles.len() == 1 {
            let only = group.tiles.remove(0);
            *self = only;
            return;
        }
        let direction = group.direction;
        let nested =
            |tile: &Tile| matches!(tile, Tile::Group(inner) if inner.direction == direction);
        if !group.tiles.iter().any(nested) {
            return;
        }
        for tile in std::mem::take(&mut group.tiles) {
            match tile {
                Tile::Group(inner) if inner.direction == direction => {
                    group.tiles.extend(inner.tiles)
                }
                other => group.tiles.push(other),
            }
        }
        group.reset_sizes();
    }

    fn place(&self, area: Rect, placement: &mut Placement) {
        match self {
            Tile::Pane(pane) => {
                let (title, body) = area.split_top(1);
                placement.panes.push(PaneArea {
                    id: pane.id,
                    title,
                    body,
                });
            }
            Tile::Group(group) => {
                for (tile, tile_area) in group.tiles.iter().zip(group.areas(area)) {
                    if group.direction == Direction::SideBySide && tile_area.x > area.x {
                        let separator = Rect::new(tile_area.x - 1, area.y, 1, area.h);
                        placement.separators.push(separator);
                    }
                    tile.place(tile_area, placement);
                }
            }
        }
    }

    /// Resizes the innermost group of `direction` around pane `id`.
    fn resize(&mut self, id: usize, direction: Direction, amount: i32, area: Rect) -> Option<i32> {
        let Tile::Group(group) = self else {
            return None;
        };
        let index = group.tiles.iter().position(|tile| tile.contains(id))?;
        let tile_area = group.areas(area)[index];
        let inner = group.tiles[index].resize(id, direction, amount, tile_area);
        if inner.is_some() || group.direction != direction {
            return inner;
        }
        Some(group.grow(index, amount, direction.length(area)))
    }

    fn has_group(&self, id: usize, direction: Direction) -> bool {
        let Tile::Group(group) = self else {
            return false;
        };
        group
            .tiles
            .iter()
            .find(|tile| tile.contains(id))
            .is_some_and(|tile| group.direction == direction || tile.has_group(id, direction))
    }

    fn reset_sizes(&mut self) {
        if let Tile::Group(group) = self {
            group.reset_sizes();
            group.tiles.iter_mut().for_each(Tile::reset_sizes);
        }
    }
}

impl Panes {
    pub fn is_split(&self) -> bool {
        matches!(self.root, Tile::Group(_))
    }

    /// Every pane, from left to right and top to bottom.
    pub fn all(&self) -> Vec<&Pane> {
        let mut panes = Vec::new();
        self.root.collect(&mut panes);
        panes
    }

    pub fn ids(&self) -> Vec<usize> {
        self.all().iter().map(|pane| pane.id).collect()
    }

    pub fn pane(&self, id: usize) -> Option<&Pane> {
        self.all().into_iter().find(|pane| pane.id == id)
    }

    pub fn pane_mut(&mut self, id: usize) -> Option<&mut Pane> {
        let mut panes = Vec::new();
        self.root.collect_mut(&mut panes);
        panes.into_iter().find(|pane| pane.id == id)
    }

    /// The panes without the focus.
    pub fn others(&self) -> Vec<&Pane> {
        let mut panes = self.all();
        panes.retain(|pane| pane.id != self.focused);
        panes
    }

    pub fn others_mut(&mut self) -> Vec<&mut Pane> {
        let focused = self.focused;
        let mut panes = Vec::new();
        self.root.collect_mut(&mut panes);
        panes.retain(|pane| pane.id != focused);
        panes
    }

    /// Splits the focused pane: a copy of it is placed after it and gets
    /// the focus.
    pub fn split(&mut self, direction: Direction) {
        let Some(focused) = self.pane(self.focused) else {
            return;
        };
        let pane = Pane {
            id: self.next_id,
            ..focused.clone()
        };
        self.next_id += 1;
        let id = pane.id;
        if self
            .root
            .insert_after(self.focused, direction, pane)
            .is_none()
        {
            self.focused = id;
        }
    }

    /// Closes pane `id`, unless it is the only one. Returns it with the
    /// pane next to it, which gets the focus when the closed pane had it.
    pub fn remove(&mut self, id: usize) -> Option<(Pane, usize)> {
        let (pane, neighbor) = self.root.remove(id)?;
        if self.focused == id {
            self.focused = neighbor;
        }
        if !self.is_split() {
            self.sync_scroll = false;
        }
        self.last_scroll = None;
        Some((pane, neighbor))
    }

    /// Whether the focused pane has a neighbor in `direction` to trade
    /// columns or rows with.
    pub fn resizable(&self, direction: Direction) -> bool {
        self.root.has_group(self.focused, direction)
    }

    /// Makes the focused pane `amount` columns wider (side by side) or rows
    /// taller (stacked), as far as its neighbor has room. Returns the
    /// columns or rows moved.
    pub fn resize(&mut self, direction: Direction, amount: i32, area: Rect) -> i32 {
        if !self.fits(area) {
            return 0;
        }
        self.root
            .resize(self.focused, direction, amount, area)
            .unwrap_or(0)
    }

    pub fn reset_sizes(&mut self) {
        self.root.reset_sizes();
    }

    fn placed(&self, area: Rect) -> Placement {
        let mut placement = Placement::default();
        self.root.place(area, &mut placement);
        placement
    }

    /// Whether every pane gets at least the minimum size in `area`.
    pub fn fits(&self, area: Rect) -> bool {
        self.placed(area).panes.iter().all(|pane| {
            let (width, height) = pane.size();
            width >= MIN_PANE_WIDTH && height >= MIN_PANE_HEIGHT
        })
    }

    /// Where the panes are drawn in `area`. Only the focused pane, without
    /// a title, when it is the only one or the panes do not fit.
    pub fn place(&self, area: Rect) -> Placement {
        if self.is_split() && self.fits(area) {
            return self.placed(area);
        }
        let (title, body) = area.split_top(0);
        Placement {
            panes: vec![PaneArea {
                id: self.focused,
                title,
                body,
            }],
            separators: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::new(0, 0, 80, 20);

    fn widths(panes: &Panes) -> Vec<u16> {
        let placement = panes.place(SCREEN);
        placement.panes.iter().map(|pane| pane.size().0).collect()
    }

    fn heights(panes: &Panes) -> Vec<u16> {
        let placement = panes.place(SCREEN);
        placement.panes.iter().map(|pane| pane.size().1).collect()
    }

    #[test]
    fn panes_of_a_group_share_its_space_evenly() {
        let mut panes = Panes::default();
        panes.split(Direction::SideBySide);
        assert_eq!(widths(&panes), [39, 40]);
        panes.split(Direction::SideBySide);
        assert_eq!(widths(&panes), [26, 26, 26]);
        assert_eq!(panes.ids(), [0, 1, 2]);
        assert_eq!(panes.focused, 2);
        assert_eq!(panes.place(SCREEN).separators.len(), 2);
    }

    #[test]
    fn splitting_the_other_way_nests_a_group() {
        let mut panes = Panes::default();
        panes.split(Direction::SideBySide);
        panes.split(Direction::Stacked);
        assert_eq!(widths(&panes), [39, 40, 40]);
        assert_eq!(heights(&panes), [20, 10, 10]);
        panes.split(Direction::Stacked);
        assert_eq!(heights(&panes), [20, 6, 7, 7]);
    }

    #[test]
    fn closing_a_pane_focuses_its_neighbor_and_flattens_groups() {
        let mut panes = Panes::default();
        panes.split(Direction::SideBySide);
        panes.split(Direction::Stacked);
        panes.split(Direction::SideBySide);
        assert_eq!(panes.ids(), [0, 1, 2, 3]);
        panes.focused = 1;
        let (closed, neighbor) = panes.remove(1).unwrap();
        assert_eq!((closed.id, neighbor, panes.focused), (1, 2, 2));
        assert_eq!(widths(&panes), [26, 26, 26]);
        panes.remove(2);
        panes.remove(3);
        assert!(!panes.is_split());
        assert!(panes.remove(0).is_none());
    }

    #[test]
    fn a_resize_moves_columns_between_neighbors_until_the_minimum() {
        let mut panes = Panes::default();
        panes.split(Direction::SideBySide);
        panes.split(Direction::SideBySide);
        panes.focused = 0;
        assert_eq!(panes.resize(Direction::SideBySide, 10, SCREEN), 10);
        assert_eq!(widths(&panes), [36, 16, 26]);
        assert_eq!(panes.resize(Direction::SideBySide, 100, SCREEN), 6);
        assert_eq!(widths(&panes), [42, 10, 26]);
        assert_eq!(panes.resize(Direction::SideBySide, -100, SCREEN), -32);
        assert_eq!(widths(&panes), [10, 42, 26]);
        assert_eq!(panes.resize(Direction::Stacked, 3, SCREEN), 0);
        panes.focused = 2;
        assert_eq!(panes.resize(Direction::SideBySide, 4, SCREEN), 4);
        assert_eq!(widths(&panes), [10, 38, 30]);
        panes.reset_sizes();
        assert_eq!(widths(&panes), [26, 26, 26]);
    }

    #[test]
    fn a_new_pane_makes_its_group_even_again() {
        let mut panes = Panes::default();
        panes.split(Direction::SideBySide);
        panes.resize(Direction::SideBySide, 10, SCREEN);
        assert_eq!(widths(&panes), [29, 50]);
        panes.split(Direction::SideBySide);
        assert_eq!(widths(&panes), [26, 26, 26]);
    }

    #[test]
    fn a_nested_group_is_resized_as_one_tile() {
        let mut panes = Panes::default();
        panes.split(Direction::SideBySide);
        panes.split(Direction::Stacked);
        assert!(panes.resizable(Direction::SideBySide));
        assert!(panes.resizable(Direction::Stacked));
        assert_eq!(panes.resize(Direction::SideBySide, 5, SCREEN), 5);
        assert_eq!(widths(&panes), [34, 45, 45]);
        assert_eq!(panes.resize(Direction::Stacked, -4, SCREEN), -4);
        assert_eq!(heights(&panes), [20, 14, 6]);
        panes.focused = 0;
        assert!(!panes.resizable(Direction::Stacked));
    }

    #[test]
    fn the_neighbor_on_a_side_is_level_with_the_cursor() {
        let mut panes = Panes::default();
        panes.split(Direction::SideBySide);
        panes.split(Direction::Stacked);
        panes.split(Direction::SideBySide);
        let areas = panes.place(SCREEN).panes;
        let next = |from, side, near| neighbor(&areas, from, side, near);
        assert_eq!(next(0, Side::Right, (5, 3)), Some(1));
        assert_eq!(next(0, Side::Right, (5, 15)), Some(2));
        assert_eq!(next(0, Side::Left, (5, 3)), None);
        assert_eq!(next(0, Side::Below, (5, 3)), None);
        assert_eq!(next(1, Side::Below, (45, 3)), Some(2));
        assert_eq!(next(1, Side::Below, (70, 3)), Some(3));
        assert_eq!(next(1, Side::Left, (45, 3)), Some(0));
        assert_eq!(next(3, Side::Left, (70, 15)), Some(2));
        assert_eq!(next(3, Side::Above, (70, 15)), Some(1));
        assert_eq!(next(3, Side::Right, (70, 15)), None);
    }

    #[test]
    fn panes_that_do_not_fit_leave_the_focused_one() {
        let mut panes = Panes::default();
        panes.split(Direction::SideBySide);
        let narrow = Rect::new(0, 0, 20, 20);
        assert!(!panes.fits(narrow));
        let placement = panes.place(narrow);
        assert_eq!(placement.panes.len(), 1);
        assert_eq!(placement.panes[0].id, 1);
        assert_eq!(placement.panes[0].body, narrow);
    }
}
