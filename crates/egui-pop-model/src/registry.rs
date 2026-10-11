//! An owner-tagged registry: items by id, each with the owner that
//! registered it, removable by owner in one call. Every registry of the
//! model is built on it, so a plugin's registrations go away with it
//! (ADR 0004).

use std::collections::BTreeMap;

use crate::ids::OwnerId;

/// A registration refused because the id is taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken<K> {
    pub id: K,
    /// Who holds it.
    pub by: OwnerId,
}

/// One registered item.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot<T> {
    pub owner: OwnerId,
    /// The order it was registered in, across the registry.
    pub order: u64,
    pub item: T,
}

/// Items by id, each owned.
#[derive(Clone, Debug)]
pub struct Registry<K: Ord + Clone, T> {
    slots: BTreeMap<K, Slot<T>>,
    next: u64,
}

impl<K: Ord + Clone, T> Default for Registry<K, T> {
    fn default() -> Self {
        Self {
            slots: BTreeMap::new(),
            next: 0,
        }
    }
}

impl<K: Ord + Clone, T> Registry<K, T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an item; refused when the id is taken.
    pub fn insert(&mut self, owner: &OwnerId, id: K, item: T) -> Result<(), Taken<K>> {
        if let Some(s) = self.slots.get(&id) {
            return Err(Taken {
                id,
                by: s.owner.clone(),
            });
        }
        let order = self.next;
        self.next += 1;
        self.slots.insert(
            id,
            Slot {
                owner: owner.clone(),
                order,
                item,
            },
        );
        Ok(())
    }

    pub fn get(&self, id: &K) -> Option<&T> {
        self.slots.get(id).map(|s| &s.item)
    }

    pub fn get_mut(&mut self, id: &K) -> Option<&mut T> {
        self.slots.get_mut(id).map(|s| &mut s.item)
    }

    pub fn slot(&self, id: &K) -> Option<&Slot<T>> {
        self.slots.get(id)
    }

    pub fn owner(&self, id: &K) -> Option<&OwnerId> {
        self.slots.get(id).map(|s| &s.owner)
    }

    pub fn contains(&self, id: &K) -> bool {
        self.slots.contains_key(id)
    }

    pub fn remove(&mut self, id: &K) -> Option<Slot<T>> {
        self.slots.remove(id)
    }

    /// Removes everything an owner registered; returns it, by id.
    pub fn remove_owner(&mut self, owner: &OwnerId) -> Vec<(K, T)> {
        let ids: Vec<K> = self
            .slots
            .iter()
            .filter(|(_, s)| s.owner == *owner)
            .map(|(k, _)| k.clone())
            .collect();
        ids.into_iter()
            .filter_map(|k| self.slots.remove(&k).map(|s| (k, s.item)))
            .collect()
    }

    /// Items by id.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &Slot<T>)> {
        self.slots.iter()
    }

    /// Items in registration order.
    pub fn in_order(&self) -> Vec<(&K, &Slot<T>)> {
        let mut v: Vec<_> = self.slots.iter().collect();
        v.sort_by_key(|(_, s)| s.order);
        v
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// A list of owned items without ids (menu items, bindings): order kept,
/// removable by owner.
#[derive(Clone, Debug)]
pub struct OwnedList<T> {
    items: Vec<Slot<T>>,
    next: u64,
}

impl<T> Default for OwnedList<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next: 0,
        }
    }
}

impl<T> OwnedList<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, owner: &OwnerId, item: T) -> u64 {
        let order = self.next;
        self.next += 1;
        self.items.push(Slot {
            owner: owner.clone(),
            order,
            item,
        });
        order
    }

    pub fn iter(&self) -> impl Iterator<Item = &Slot<T>> {
        self.items.iter()
    }

    pub fn retain(&mut self, f: impl FnMut(&Slot<T>) -> bool) {
        self.items.retain(f);
    }

    /// Removes an owner's items; returns how many.
    pub fn remove_owner(&mut self, owner: &OwnerId) -> usize {
        let before = self.items.len();
        self.items.retain(|s| s.owner != *owner);
        before - self.items.len()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_owned_and_removed_by_owner() {
        let a = OwnerId::from_static("a");
        let b = OwnerId::from_static("b");
        let mut r: Registry<String, u32> = Registry::new();
        r.insert(&a, "x".into(), 1).unwrap();
        r.insert(&b, "y".into(), 2).unwrap();
        r.insert(&a, "z".into(), 3).unwrap();
        let taken = r.insert(&b, "x".into(), 9).unwrap_err();
        assert_eq!(taken.by, a);
        let order: Vec<&String> = r.in_order().into_iter().map(|(k, _)| k).collect();
        assert_eq!(order, ["x", "y", "z"]);
        let gone = r.remove_owner(&a);
        assert_eq!(gone.len(), 2);
        assert_eq!(r.len(), 1);
        assert_eq!(r.owner(&"y".to_string()), Some(&b));

        let mut l = OwnedList::new();
        l.push(&a, 1);
        l.push(&b, 2);
        l.push(&a, 3);
        assert_eq!(l.remove_owner(&a), 2);
        assert_eq!(l.iter().map(|s| s.item).collect::<Vec<_>>(), [2]);
    }
}
