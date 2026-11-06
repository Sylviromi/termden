use serde::{Deserialize, Serialize};

use crate::model::HarnessId;

/// Split direction. `Horizontal` places children side by side (a vertical
/// divider); `Vertical` stacks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropZone {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

impl DropZone {
    pub fn is_center(self) -> bool {
        matches!(self, DropZone::Center)
    }
}

/// Binary dwindle tree. Leaves reference harnesses that live in the same
/// workspace; the tree only describes geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutNode {
    Leaf(HarnessId),
    Split {
        axis: Axis,
        /// Share of the available space given to `first`, between 0.1 and 0.9.
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn leaf(id: HarnessId) -> Self {
        Self::Leaf(id)
    }

    pub fn split(axis: Axis, ratio: f32, first: LayoutNode, second: LayoutNode) -> Self {
        Self::Split {
            axis,
            ratio: ratio.clamp(0.1, 0.9),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    pub fn leaves(&self) -> Vec<HarnessId> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, out: &mut Vec<HarnessId>) {
        match self {
            LayoutNode::Leaf(id) => out.push(*id),
            LayoutNode::Split { first, second, .. } => {
                first.collect_leaves(out);
                second.collect_leaves(out);
            }
        }
    }

    pub fn len(&self) -> usize {
        match self {
            LayoutNode::Leaf(_) => 1,
            LayoutNode::Split { first, second, .. } => first.len() + second.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn contains(&self, id: HarnessId) -> bool {
        match self {
            LayoutNode::Leaf(leaf) => *leaf == id,
            LayoutNode::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    pub fn first_leaf(&self) -> Option<HarnessId> {
        match self {
            LayoutNode::Leaf(id) => Some(*id),
            LayoutNode::Split { first, .. } => first.first_leaf(),
        }
    }

    /// Insert `new` next to `target`, splitting in the requested direction.
    pub fn split_at(&mut self, target: HarnessId, new: HarnessId, zone: DropZone) -> bool {
        if zone.is_center() {
            return false;
        }

        match self {
            LayoutNode::Leaf(id) if *id == target => {
                let old = self.clone();
                *self = match zone {
                    DropZone::Left => Self::split(Axis::Horizontal, 0.5, Self::leaf(new), old),
                    DropZone::Right => Self::split(Axis::Horizontal, 0.5, old, Self::leaf(new)),
                    DropZone::Top => Self::split(Axis::Vertical, 0.5, Self::leaf(new), old),
                    DropZone::Bottom => Self::split(Axis::Vertical, 0.5, old, Self::leaf(new)),
                    DropZone::Center => unreachable!(),
                };
                true
            }
            LayoutNode::Leaf(_) => false,
            LayoutNode::Split { first, second, .. } => {
                first.split_at(target, new, zone) || second.split_at(target, new, zone)
            }
        }
    }

    /// Swap two leaves in place. Used for the center drop zone.
    pub fn swap(&mut self, a: HarnessId, b: HarnessId) -> bool {
        if a == b {
            return false;
        }

        let mut found_a = false;
        let mut found_b = false;
        self.for_each_leaf_mut(&mut |id| {
            if *id == a {
                *id = b;
                found_a = true;
            } else if *id == b {
                *id = a;
                found_b = true;
            }
        });
        found_a && found_b
    }

    fn for_each_leaf_mut(&mut self, f: &mut impl FnMut(&mut HarnessId)) {
        match self {
            LayoutNode::Leaf(id) => f(id),
            LayoutNode::Split { first, second, .. } => {
                first.for_each_leaf_mut(f);
                second.for_each_leaf_mut(f);
            }
        }
    }

    /// Remove a leaf, collapsing its parent split. Returns false for the root
    /// leaf; callers should keep at least one leaf alive.
    pub fn remove(&mut self, id: HarnessId) -> bool {
        let replacement = match self {
            LayoutNode::Leaf(_) => return false,
            LayoutNode::Split { first, second, .. } => {
                if matches!(first.as_ref(), LayoutNode::Leaf(leaf) if *leaf == id) {
                    Some((**second).clone())
                } else if matches!(second.as_ref(), LayoutNode::Leaf(leaf) if *leaf == id) {
                    Some((**first).clone())
                } else if first.remove(id) || second.remove(id) {
                    return true;
                } else {
                    None
                }
            }
        };

        match replacement {
            Some(node) => {
                *self = node;
                true
            }
            None => false,
        }
    }

    /// Set the ratio of the split at `path` (`false` = first child,
    /// `true` = second, from the root). Returns false if there is none.
    pub fn set_ratio(&mut self, path: &[bool], value: f32) -> bool {
        match (self, path) {
            (LayoutNode::Split { ratio, .. }, []) => {
                *ratio = value.clamp(0.1, 0.9);
                true
            }
            (LayoutNode::Split { first, second, .. }, [side, rest @ ..]) => {
                if *side { second } else { first }.set_ratio(rest, value)
            }
            (LayoutNode::Leaf(_), _) => false,
        }
    }

    /// Remove `dragged` and re-insert it relative to `target`.
    pub fn move_leaf(&mut self, dragged: HarnessId, target: HarnessId, zone: DropZone) -> bool {
        if dragged == target {
            return false;
        }
        if self.len() < 2 || !self.contains(target) {
            return false;
        }

        if zone.is_center() {
            return self.swap(dragged, target);
        }

        self.remove(dragged);
        if !self.contains(target) {
            return false;
        }
        self.split_at(target, dragged, zone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_ratio_follows_path() {
        let id = HarnessId::new;
        let inner = LayoutNode::split(
            Axis::Vertical,
            0.5,
            LayoutNode::leaf(id(2)),
            LayoutNode::leaf(id(3)),
        );
        let mut root = LayoutNode::split(Axis::Horizontal, 0.5, LayoutNode::leaf(id(1)), inner);
        assert!(root.set_ratio(&[true], 0.3));
        assert!(root.set_ratio(&[], 2.0));
        assert!(!root.set_ratio(&[false], 0.4));
        let LayoutNode::Split { ratio, second, .. } = &root else {
            panic!()
        };
        assert_eq!(*ratio, 0.9);
        assert!(matches!(**second, LayoutNode::Split { ratio, .. } if ratio == 0.3));
    }
}
