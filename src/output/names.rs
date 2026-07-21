use std::cell::UnsafeCell;
use std::rc::Rc;

#[derive(Default)]
struct SeqMap<T> {
    data: Box<[Option<T>]>,
}

impl<T> SeqMap<T> {
    fn get(&mut self, index: usize, init: impl FnOnce() -> T) -> &mut T {
        if index >= self.data.len() {
            let mut vec = std::mem::take(&mut self.data).into_vec();
            vec.reserve(index + 1 - vec.capacity());
            vec.resize_with(vec.capacity(), || None);
            self.data = vec.into_boxed_slice();
        }

        let item = &mut self.data[index];
        if item.is_none() {
            *item = Some(init());
        }
        item.as_mut().unwrap()
    }
}

pub type Name = Rc<str>;

macro_rules! fmt_name {
    ($name:expr, $val:expr) => {
        Rc::from(format!("moissanite_{}_{}", $name, $val))
    };
}

#[derive(Default)]
struct NameManager {
    functions: SeqMap<Name>,
    tables: SeqMap<Name>,
    memories: SeqMap<Name>,
    globals: SeqMap<Name>,
    locals: SeqMap<Name>,
    stack: SeqMap<Name>,
}

thread_local! {
    static MANAGER: UnsafeCell<NameManager> = UnsafeCell::new(NameManager::default());
    pub static I: Name = Rc::from("i");
    pub static TMP: Name = Rc::from("tmp");
    pub static INIT_FN: Name = Rc::from("moissanite_init");
    pub static BLANK_PAGE: Name = Rc::from("moissanite_blank");
    pub static PAGE_SIZE: Name = Rc::from("8192");
    pub static ZERO: Name = Rc::from("0");
}

pub fn function(i: usize) -> Name {
    MANAGER.with(|names| unsafe { names.get().as_mut_unchecked() }.functions.get(i, || fmt_name!("function", i)).clone())
}
pub fn table(i: usize) -> Name {
    MANAGER.with(|names| unsafe { names.get().as_mut_unchecked() }.tables.get(i, || fmt_name!("table", i)).clone())
}
pub fn memory(i: usize) -> Name {
    MANAGER.with(|names| unsafe { names.get().as_mut_unchecked() }.memories.get(i, || fmt_name!("memory", i)).clone())
}
pub fn global(i: usize) -> Name {
    MANAGER.with(|names| unsafe { names.get().as_mut_unchecked() }.globals.get(i, || fmt_name!("global", i)).clone())
}
pub fn local(i: usize) -> Name {
    MANAGER.with(|names| unsafe { names.get().as_mut_unchecked() }.locals.get(i, || fmt_name!("local", i)).clone())
}
pub fn stack(i: usize) -> Name {
    MANAGER.with(|names| unsafe { names.get().as_mut_unchecked() }.stack.get(i, || fmt_name!("stack", i)).clone())
}