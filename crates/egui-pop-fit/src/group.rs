//! Groups: runs of consecutive elements packed apart from the rest, like
//! the controls of an HTML `fieldset` under its legend.

use crate::layout::{GridRect, PointRect};
use core::ops::Range;

/// A run of consecutive elements packed apart from the rest.
///
/// Packing stays dense within a group, but order is strict across groups:
/// a group starts on a new band of rows below everything before it, and
/// nothing after it moves up into one of its gaps. Loose elements (in no
/// group) between two groups form a band of their own, packed densely
/// too.
///
/// A group may hold groups, to any depth, as long as each lies wholly
/// within its parent's members. An empty group holds nothing and draws no
/// frame, but still splits the loose elements around it into two bands:
/// it is a barrier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Group {
    /// The caller's key for the group.
    pub key: u64,
    /// Its elements, as a range of indices into the element slice.
    pub range: Range<usize>,
    /// Whether its first element is a header (a legend), placed alone on
    /// the group's first band, before its other members.
    pub header: bool,
}

impl Group {
    /// A group of the elements in `range`, with no header.
    pub fn new(key: u64, range: Range<usize>) -> Group {
        Group {
            key,
            range,
            header: false,
        }
    }

    /// The same group with its first element as its header.
    pub fn with_header(mut self) -> Group {
        self.header = true;
        self
    }

    /// An empty group at `index`: a barrier between the elements before
    /// it and from it on.
    pub fn barrier(key: u64, index: usize) -> Group {
        Group::new(key, index..index)
    }

    /// The index of its header element, if it has one.
    pub(crate) fn header_index(&self) -> Option<usize> {
        (self.header && !self.range.is_empty()).then_some(self.range.start)
    }

    /// The first index of its members (after its header).
    pub(crate) fn members_start(&self) -> usize {
        self.range.start + usize::from(self.header_index().is_some())
    }
}

/// The result for one group.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GroupPlacement {
    /// The group's key.
    pub key: u64,
    /// The index, in the layout's groups, of the group holding it.
    pub parent: Option<usize>,
    /// Its band, in twelfths: the grid's whole width, from the top of its
    /// first placed element (its header, if any) to the bottom of its
    /// lowest one. `None` when nothing of it is placed.
    pub rect: Option<GridRect>,
    /// Its band, in points.
    pub points: Option<PointRect>,
    /// The row, in twelfths, its members' pins count from: just below its
    /// header.
    pub body_top: u32,
    /// Set when the group was ignored: it reaches past the elements, or
    /// it crosses another group's boundary or header. Its elements are
    /// laid out as if it were not there.
    pub ignored: bool,
}

/// A node of the group tree: an element, or a group of nodes.
#[derive(Debug)]
pub(crate) enum Node {
    Element(usize),
    Group {
        /// Its index in the caller's groups.
        index: usize,
        header: Option<usize>,
        children: Vec<Node>,
    },
}

/// The tree of `count` elements under `groups`, and which groups were
/// ignored. Each group's parent is the innermost valid group holding it.
pub(crate) fn tree(count: usize, groups: &[Group]) -> (Vec<Node>, Vec<Option<usize>>, Vec<bool>) {
    let mut order: Vec<usize> = (0..groups.len()).collect();
    // Outer before inner: by start, then by the larger end.
    order.sort_by_key(|&g| {
        let r = &groups[g].range;
        (r.start, core::cmp::Reverse(r.end), g)
    });
    let mut parent = vec![None; groups.len()];
    let mut ignored = vec![false; groups.len()];
    let mut stack: Vec<usize> = Vec::new();
    for &g in &order {
        let r = &groups[g].range;
        if r.start > r.end || r.end > count {
            ignored[g] = true;
            continue;
        }
        // Close the groups that end before this one starts.
        while let Some(&top) = stack.last() {
            if r.start >= groups[top].range.end && !inside(&groups[g], &groups[top]) {
                stack.pop();
            } else {
                break;
            }
        }
        match stack.last() {
            Some(&top) if !inside(&groups[g], &groups[top]) => ignored[g] = true,
            top => {
                parent[g] = top.copied();
                stack.push(g);
            }
        }
    }
    let nodes = children(0, count, None, &order, groups, &parent, &ignored);
    (nodes, parent, ignored)
}

/// Whether `g` lies within `outer`'s members.
fn inside(g: &Group, outer: &Group) -> bool {
    g.range.start >= outer.members_start() && g.range.end <= outer.range.end
}

/// The nodes for elements `lo..hi`, whose groups have parent `parent`.
fn children(
    lo: usize,
    hi: usize,
    parent: Option<usize>,
    order: &[usize],
    groups: &[Group],
    parents: &[Option<usize>],
    ignored: &[bool],
) -> Vec<Node> {
    let mut kids = order
        .iter()
        .copied()
        .filter(|&g| !ignored[g] && parents[g] == parent)
        .peekable();
    let mut nodes = Vec::new();
    let mut i = lo;
    loop {
        if let Some(&g) = kids.peek() {
            if groups[g].range.start <= i {
                kids.next();
                let group = &groups[g];
                nodes.push(Node::Group {
                    index: g,
                    header: group.header_index(),
                    children: children(
                        group.members_start(),
                        group.range.end,
                        Some(g),
                        order,
                        groups,
                        parents,
                        ignored,
                    ),
                });
                i = i.max(group.range.end);
                continue;
            }
        }
        if i < hi {
            nodes.push(Node::Element(i));
            i += 1;
        } else {
            break;
        }
    }
    nodes
}
