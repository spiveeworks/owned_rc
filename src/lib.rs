use std::cell;
use std::rc;

mod refs;

pub use refs::Ref;
pub use refs::RefMut;

pub struct Owned<T> (rc::Rc<cell::RefCell<T>>);
pub struct Link<T> (rc::Weak<cell::RefCell<T>>);

pub enum BorrowError {
    Missing,
    Busy(cell::BorrowError),
}

pub enum BorrowMutError {
    Missing,
    Busy(cell::BorrowMutError),
}


impl From<cell::BorrowError> for BorrowError {
    fn from(value: cell::BorrowError) -> Self {
        BorrowError::Busy(value)
    }
}

impl From<cell::BorrowMutError> for BorrowMutError {
    fn from(value: cell::BorrowMutError) -> Self {
        BorrowMutError::Busy(value)
    }
}

impl<T> Clone for Link<T> {
    fn clone(&self) -> Self {
        Link(self.0.clone())
    }
}



impl<T> Owned<T> {
    pub fn new(value: T) -> Self {
        // where's point-free when you need it
        Owned(rc::Rc::new(cell::RefCell::new(value)))
    }

    pub fn share(&self) -> Link<T> {
        Link(rc::Rc::downgrade(&self.0))
    }

    pub fn try_borrow(ptr: &Self) -> Result<Ref<T>, cell::BorrowError> {
        let strong = ptr.0.clone();
        Ok(Ref::new(strong)?)
    }

    pub fn try_borrow_mut(ptr: &Self) -> Result<RefMut<T>, cell::BorrowMutError> {
        let strong = ptr.0.clone();
        Ok(RefMut::new(strong)?)
    }
}


impl<T> Link<T> {
    // create an empty link, useful when initalizing cycles
    pub fn new() -> Self {
        Link(rc::Weak::new())
    }


    pub fn try_borrow(&self) -> Result<Ref<T>, BorrowError> {
        let strong = self.0
                         .upgrade()
                         .ok_or(BorrowError::Missing)?;
        Ok(Ref::new(strong)?)
    }

    pub fn try_borrow_mut(self: &Self) -> Result<RefMut<T>, BorrowMutError> {
        let strong = self.0
                         .upgrade()
                         .ok_or(BorrowMutError::Missing)?;
        Ok(RefMut::new(strong)?)
    }
}

