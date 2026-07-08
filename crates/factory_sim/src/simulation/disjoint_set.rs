use std::collections::BTreeMap;

#[derive(Clone)]
pub(in crate::simulation) struct DisjointSet {
    parents: Vec<usize>,
    ranks: Vec<u8>,
}

impl DisjointSet {
    pub(in crate::simulation) fn new(size: usize) -> Self {
        Self {
            parents: (0..size).collect(),
            ranks: vec![0; size],
        }
    }

    pub(in crate::simulation) fn find(&mut self, index: usize) -> usize {
        if self.parents[index] != index {
            self.parents[index] = self.find(self.parents[index]);
        }
        self.parents[index]
    }

    pub(in crate::simulation) fn union(&mut self, first: usize, second: usize) {
        let first_root = self.find(first);
        let second_root = self.find(second);
        if first_root == second_root {
            return;
        }

        match self.ranks[first_root].cmp(&self.ranks[second_root]) {
            std::cmp::Ordering::Less => self.parents[first_root] = second_root,
            std::cmp::Ordering::Greater => self.parents[second_root] = first_root,
            std::cmp::Ordering::Equal => {
                self.parents[second_root] = first_root;
                self.ranks[first_root] += 1;
            }
        }
    }

    pub(in crate::simulation) fn components(&mut self) -> BTreeMap<usize, Vec<usize>> {
        let mut components = BTreeMap::<usize, Vec<usize>>::new();
        for index in 0..self.parents.len() {
            let root = self.find(index);
            components.entry(root).or_default().push(index);
        }
        components
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unioning_connected_indices_returns_expected_components() {
        let mut set = DisjointSet::new(5);
        set.union(0, 1);
        set.union(1, 2);
        set.union(3, 4);

        let components = sorted_components(set.components());

        assert_eq!(components, vec![vec![0, 1, 2], vec![3, 4]]);
    }

    #[test]
    fn disconnected_indices_remain_separate() {
        let mut set = DisjointSet::new(3);

        let components = sorted_components(set.components());

        assert_eq!(components, vec![vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn repeated_unions_are_idempotent() {
        let mut set = DisjointSet::new(4);
        set.union(0, 1);
        set.union(0, 1);
        set.union(1, 0);
        set.union(2, 3);
        set.union(2, 3);

        let components = sorted_components(set.components());

        assert_eq!(components, vec![vec![0, 1], vec![2, 3]]);
    }

    fn sorted_components(components: BTreeMap<usize, Vec<usize>>) -> Vec<Vec<usize>> {
        let mut components = components.into_values().collect::<Vec<_>>();
        components.sort();
        components
    }
}
