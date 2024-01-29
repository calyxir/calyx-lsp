use tree_sitter::Node;

pub trait ParentUntil: Sized {
    fn parent_until<F>(&self, pred: F) -> Option<Self>
    where
        F: Fn(&Self) -> bool;
}

impl ParentUntil for Node<'_> {
    fn parent_until<F>(&self, pred: F) -> Option<Self>
    where
        F: Fn(&Self) -> bool,
    {
        self.parent().and_then(|parent| {
            if pred(&parent) {
                Some(parent)
            } else {
                parent.parent_until(pred)
            }
        })
    }
}
