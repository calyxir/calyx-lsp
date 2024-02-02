use tree_sitter::Node;

pub trait ParentUntil: Sized {
    fn parent_until<F>(&self, pred: F) -> Option<Self>
    where
        F: Fn(&Self) -> bool;

    fn parent_until_name<S>(&self, name: S) -> Option<Self>
    where
        S: AsRef<str>;
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

    fn parent_until_name<S>(&self, name: S) -> Option<Self>
    where
        S: AsRef<str>,
    {
        self.parent_until(|p| p.kind() == name.as_ref())
    }
}
