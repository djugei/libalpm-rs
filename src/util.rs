#![allow(dead_code)]
use std::cell::OnceCell;

#[derive(Default)]
pub struct StableList<T> {
    first: OnceCell<Box<ListElement<T>>>,
    // Could put an (opt) reference to the last element here
    // would speed up to O(1) from O(N) on the inserts.
    // But tbf it barely matters, this is expected to hold like 3 elements.
}

struct ListElement<T> {
    value: T,
    next: OnceCell<Box<Self>>,
}

impl<T> ListElement<T> {
    fn new(v: T) -> Self {
        ListElement {
            value: v,
            next: OnceCell::new(),
        }
    }
}

impl<T> StableList<T> {
    fn push(&self, element: T) -> &T {
        let mut cur = &self.first;
        while let Some(next) = cur.get() {
            cur = &next.next;
        }
        &cur.get_or_init(|| Box::new(ListElement::new(element)))
            .value
    }
}

#[test]
fn test_push() {
    let list = StableList::default();
    let v1 = list.push("once");
    let v2 = list.push("twice");
    let v3 = list.push("thrice");

    println!("{v3} {v2} {v1}");
}
