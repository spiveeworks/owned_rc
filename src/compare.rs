use std::cell;
use std::mem;
use std::rc;

enum PtrCompareEnum<'a, T: 'a + ?Sized> {
    Raw(*const T),
    Borrow(&'a rc::Rc<T>),
}

impl<'a, T: 'a + ?Sized> PtrCompareEnum<'a, T> {
    fn using_borrow<F, R>(&self, f: F) -> R where F: FnOnce(&rc::Rc<T>) -> R {
        unsafe {
            match *self {
                PtrCompareEnum::Raw(ptr) => {
                    let as_rc = mem::ManuallyDrop::new(rc::Rc::from_raw(ptr));
                    f(&*as_rc)
                },
                PtrCompareEnum::Borrow(borrow_rc) => f(borrow_rc),
            }
        }
    }
}


impl<'a, T: 'a + ?Sized> PartialEq for PtrCompareEnum<'a, T> {
    fn eq(self: &PtrCompareEnum<'a, T>, other: &PtrCompareEnum<'a, T>) -> bool {
        self.using_borrow(|x| {
            other.using_borrow(|y| {
                rc::Rc::ptr_eq(x, y)
            })
        })
    }
}

impl<'a, T: 'a + ?Sized> Eq for PtrCompareEnum<'a, T> {}


pub struct PtrCompare<'a, T: 'a + ?Sized>(PtrCompareEnum<'a, cell::RefCell<T>>);

impl<'a, T: 'a + ?Sized> PtrCompare<'a, T> {
    pub(crate) fn from_raw(ptr: *const cell::RefCell<T>) -> Self {
        PtrCompare(PtrCompareEnum::Raw(ptr))
    }

    pub(crate) fn from_rc(rc_borrow: &'a rc::Rc<cell::RefCell<T>>) -> Self {
        PtrCompare(PtrCompareEnum::Borrow(rc_borrow))
    }
}

impl<'a, T: 'a + ?Sized> PartialEq for PtrCompare<'a, T> {
    fn eq(self: &Self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<'a, T: 'a + ?Sized> Eq for PtrCompare<'a, T> {}
