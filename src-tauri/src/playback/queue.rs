//! キュー（キュー追加・次に再生・削除・並び替え・全削除）。状態は曲 ID の列だけ。

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Queue {
    items: Vec<i64>,
}

impl Queue {
    pub fn from_ids(ids: Vec<i64>) -> Self {
        Self { items: ids }
    }
    pub fn ids(&self) -> &[i64] {
        &self.items
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    /// キュー追加：最後に足す
    pub fn add(&mut self, ids: &[i64]) {
        self.items.extend_from_slice(ids);
    }
    /// 次に再生：先頭に、渡した順のまま差し込む
    pub fn play_next(&mut self, ids: &[i64]) {
        let mut v = ids.to_vec();
        v.extend(self.items.drain(..));
        self.items = v;
    }
    /// 削除（位置で指定。範囲外は無視）
    pub fn remove_at(&mut self, positions: &[usize]) {
        let mut ps: Vec<usize> = positions.iter().copied().filter(|p| *p < self.items.len()).collect();
        ps.sort_unstable();
        ps.dedup();
        for p in ps.into_iter().rev() {
            self.items.remove(p);
        }
    }
    /// 並び替え
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.items.len() {
            return;
        }
        let v = self.items.remove(from);
        let to = to.min(self.items.len());
        self.items.insert(to, v);
    }
    /// 全削除
    pub fn clear(&mut self) {
        self.items.clear();
    }
    /// 次の曲として取り出す
    pub fn pop_front(&mut self) -> Option<i64> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.remove(0))
        }
    }
    /// ライブラリから消えた曲を外す
    pub fn retain_existing(&mut self, exists: impl Fn(i64) -> bool) {
        self.items.retain(|id| exists(*id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_play_next() {
        let mut q = Queue::default();
        q.add(&[1, 2]);
        q.play_next(&[9, 8]);
        assert_eq!(q.ids(), &[9, 8, 1, 2]);
        assert_eq!(q.pop_front(), Some(9));
        assert_eq!(q.ids(), &[8, 1, 2]);
    }

    #[test]
    fn remove_move_clear() {
        let mut q = Queue::from_ids(vec![1, 2, 3, 4, 5]);
        q.remove_at(&[4, 0, 0, 99]);
        assert_eq!(q.ids(), &[2, 3, 4]);
        q.move_item(0, 2);
        assert_eq!(q.ids(), &[3, 4, 2]);
        q.move_item(2, 0);
        assert_eq!(q.ids(), &[2, 3, 4]);
        q.move_item(9, 0);
        assert_eq!(q.ids(), &[2, 3, 4]);
        q.clear();
        assert!(q.is_empty());
        assert_eq!(q.pop_front(), None);
    }
}
